use hydrus_core::{Sha256, Tag};
use hydrus_store::{
    Store,
    content::{DomainRoles, MappingAction},
};
use serde_json::Value;
pub fn seed(store: &Store, recorded: &Value) {
    let corpus = recorded["corpus"].clone();
    let roles = DomainRoles::new(&store.snapshot().services).unwrap();
    store.write_and_refresh(move |ctx| {
        let conn=ctx.conn();
        for service in roles.local.iter().copied().chain([roles.combined_local_media,roles.local_file_storage,roles.trash]) {
            conn.execute("DELETE FROM file_domain_current WHERE service_id=?",[service])?;
            conn.execute("DELETE FROM file_domain_deleted WHERE service_id=?",[service])?;
        }
        conn.execute("DELETE FROM file_inbox",[])?;conn.execute("DELETE FROM file_archived",[])?;
        for (i,row) in corpus.as_array().unwrap().iter().enumerate() {
            let hash:Sha256=row["hash"].as_str().unwrap().parse().unwrap();
            let id=hydrus_store::master::hash_id(conn,&hash)?.unwrap();
            let imported=row["imported"].as_i64().unwrap();let deleted=row["deleted"].as_i64();
            if i==2 {conn.execute("INSERT INTO file_domain_deleted(service_id,hash_id,deleted_ms,original_added_ms) VALUES(?1,?2,?3,?4)",rusqlite::params![roles.local_file_storage,id,deleted,imported])?;}
            else {conn.execute("INSERT INTO file_domain_current(service_id,hash_id,added_ms) VALUES(?1,?2,?3)",rusqlite::params![roles.local_file_storage,id,imported])?;}
            for service in roles.local.iter().copied().chain([roles.combined_local_media]) {
                if deleted.is_some() {conn.execute("INSERT INTO file_domain_deleted(service_id,hash_id,deleted_ms,original_added_ms) VALUES(?1,?2,?3,?4)",rusqlite::params![service,id,deleted,imported])?;}
                else {conn.execute("INSERT INTO file_domain_current(service_id,hash_id,added_ms) VALUES(?1,?2,?3)",rusqlite::params![service,id,imported])?;}
            }
            if row["trash"].as_bool().unwrap() {conn.execute("INSERT INTO file_domain_current(service_id,hash_id,added_ms) VALUES(?1,?2,?3)",rusqlite::params![roles.trash,id,imported])?;}
            if row["inbox"].as_bool().unwrap() {conn.execute("INSERT INTO file_inbox VALUES(?)",[id])?;}
            if let Some(archived)=row["archived"].as_i64() {conn.execute("INSERT INTO file_archived VALUES(?,?)",rusqlite::params![id,archived])?;}
        }
        Ok(())
    }).unwrap();
    let selected = recorded["selected_current"]
        .as_array()
        .unwrap()
        .iter()
        .chain(recorded["selected_deleted"].as_array().unwrap())
        .map(|h| h.as_str().unwrap().parse::<Sha256>().unwrap())
        .collect::<Vec<_>>();
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    store
        .write_content(move |writer| {
            let tag = hydrus_store::master::intern_tag(
                writer.conn(),
                &Tag::new("parity:history-selected").unwrap(),
            )?;
            let ids = selected
                .iter()
                .map(|h| hydrus_store::master::hash_id(writer.conn(), h).map(Option::unwrap))
                .collect::<hydrus_store::Result<Vec<_>>>()?;
            writer.update_mappings(service, &MappingAction::Add, tag, &ids)
        })
        .unwrap();
}
