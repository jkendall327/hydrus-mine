//! The daemon owns API listener changes without restarting download workers.
use hydrus_api::{
    AppState,
    server::{ServerOptions, bind, serve_on},
};
use hydrus_core::ServiceType;
use hydrus_store::{
    Store,
    services::{ServerConfig, ServiceKind, ServiceRegistry},
    settings::ClientApiState,
};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{oneshot, watch};

fn configuration(store: &Store) -> hydrus_store::Result<(String, ServerConfig)> {
    store.read(|conn| {
        Ok(ServiceRegistry::load(conn)?
            .of_type(ServiceType::ClientApiService)
            .find_map(|s| match &s.kind {
                ServiceKind::ClientApi(c) => Some((s.name.clone(), c.clone())),
                _ => None,
            })
            .unwrap_or_else(|| ("client api".into(), ServerConfig::default())))
    })
}
fn options(config: &ServerConfig, port: Option<u16>, ip: Option<IpAddr>) -> Option<ServerOptions> {
    let port = port.or(config.port)?;
    let ip = ip.unwrap_or(IpAddr::V4(if config.allow_non_local_connections {
        Ipv4Addr::UNSPECIFIED
    } else {
        Ipv4Addr::LOCALHOST
    }));
    Some(ServerOptions {
        addr: SocketAddr::new(ip, port),
        cors: config.support_cors,
        log_requests: config.log_requests,
        tls: None,
    })
}
async fn changed(
    receiver: &mut watch::Receiver<(String, ServerConfig)>,
    stopped: &mut watch::Receiver<bool>,
) -> bool {
    tokio::select! {
        result=receiver.changed()=>result.is_ok(),
        ()=crate::until(stopped.clone())=>false,
    }
}
/// Serve and rebind when supported persisted settings change. Failed listeners
/// wait for a corrected config, while all other daemon jobs keep running.
pub async fn run(
    state: Arc<AppState>,
    port: Option<u16>,
    ip: Option<IpAddr>,
    mut stopped: watch::Receiver<bool>,
    say: impl Fn(ClientApiState),
) -> std::io::Result<()> {
    let initial = configuration(&state.store).map_err(std::io::Error::other)?;
    let (sender, mut receiver) = watch::channel(initial);
    let poller = tokio::spawn({
        let state = state.clone();
        let stopped = stopped.clone();
        async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                tokio::select! { ()=crate::until(stopped.clone())=>break, _=interval.tick()=>{} }
                if state.locked.load(std::sync::atomic::Ordering::SeqCst) {
                    continue;
                }
                let store = state.store.clone();
                match tokio::task::spawn_blocking(move || configuration(&store)).await {
                    Ok(Ok(fresh)) => {
                        sender.send_if_modified(|current| {
                            if *current == fresh {
                                false
                            } else {
                                *current = fresh;
                                true
                            }
                        });
                    }
                    Ok(Err(e)) => {
                        tracing::error!(error=%e,"reading Client API configuration failed");
                    }
                    Err(e) => tracing::error!(error=%e,"Client API configuration reader failed"),
                }
            }
        }
    });
    loop {
        if *stopped.borrow() {
            break;
        }
        let (name, config) = receiver.borrow_and_update().clone();
        let Some(mut options) = options(&config, port, ip) else {
            say(ClientApiState::Off);
            println!("The Client API is off: \"{name}\" has no port (--port serves it anyway)");
            if !changed(&mut receiver, &mut stopped).await {
                break;
            }
            continue;
        };
        say(ClientApiState::Starting);
        if config.use_https {
            // the pair in the db directory, made (slowly: an RSA key) on first use
            let dir = state.store.dir().to_path_buf();
            let made = tokio::task::spawn_blocking(move || hydrus_api::tls::server_config(&dir))
                .await
                .map_err(|e| e.to_string())
                .and_then(|made| made.map_err(|e| e.to_string()));
            match made {
                Ok(tls) => options.tls = Some(tls),
                Err(e) => {
                    let why = format!("Could not start \"{name}\": {e}");
                    tracing::error!("{why}; everything else runs on");
                    println!("Client API couldn't start ({why}); everything else runs on");
                    say(ClientApiState::Failed(why));
                    if !changed(&mut receiver, &mut stopped).await {
                        break;
                    }
                    continue;
                }
            }
        }
        let listener = match bind(&options).await {
            Ok(listener) => listener,
            Err(e) => {
                let why = format!("Could not start \"{name}\": {e}");
                tracing::error!("{why}; everything else runs on");
                println!("Client API couldn't start ({why}); everything else runs on");
                say(ClientApiState::Failed(why));
                if !changed(&mut receiver, &mut stopped).await {
                    break;
                }
                continue;
            }
        };
        let addr = listener.local_addr().unwrap_or(options.addr);
        say(ClientApiState::Listening(addr.to_string()));
        let scheme = if options.tls.is_some() {
            "https"
        } else {
            "http"
        };
        println!("Client API at {scheme}://{addr}");
        let (stop_listener, listener_stopped) = oneshot::channel();
        let mut server = tokio::spawn({
            let state = state.clone();
            async move {
                serve_on(listener, state, &options, async {
                    let _ = listener_stopped.await;
                })
                .await
            }
        });
        let keep_running = tokio::select! {
            keep=changed(&mut receiver,&mut stopped)=>keep,
            result=&mut server=>{let why=format!("Client API listener stopped: {result:?}");say(ClientApiState::Failed(why));changed(&mut receiver,&mut stopped).await},
        };
        let _ = stop_listener.send(());
        if !server.is_finished()
            && tokio::time::timeout(Duration::from_secs(10), &mut server)
                .await
                .is_err()
        {
            server.abort();
            let _ = server.await;
        }
        if !keep_running {
            break;
        }
    }
    poller.abort();
    let _ = poller.await;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cli_overrides_are_explicit() {
        let mut config = ServerConfig::default();
        assert!(options(&config, None, None).is_none());
        config.port = Some(45869);
        config.allow_non_local_connections = true;
        config.support_cors = true;
        config.log_requests = true;
        let configured = options(&config, None, None).unwrap();
        assert_eq!(configured.addr, "0.0.0.0:45869".parse().unwrap());
        assert!(configured.cors && configured.log_requests);
        let overridden = options(&config, Some(45999), Some("127.0.0.1".parse().unwrap())).unwrap();
        assert_eq!(overridden.addr, "127.0.0.1:45999".parse().unwrap());
    }
}
