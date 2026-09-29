//! How services are described in API responses.

use serde_json::{Map, Value as Json, json};

use hydrus_core::ServiceType;
use hydrus_store::services::{RatingColours, Service, ServiceKind, ServiceRegistry};

/// Service types the API exposes, in the order it lists them.
pub const API_SERVICE_TYPES: &[ServiceType] = &[
    ServiceType::LocalTag,
    ServiceType::TagRepository,
    ServiceType::LocalFileDomain,
    ServiceType::LocalFileUpdateDomain,
    ServiceType::FileRepository,
    ServiceType::HydrusLocalFileStorage,
    ServiceType::CombinedLocalFileDomains,
    ServiceType::CombinedFile,
    ServiceType::CombinedTag,
    ServiceType::LocalRatingLike,
    ServiceType::LocalRatingNumerical,
    ServiceType::LocalRatingIncDec,
    ServiceType::LocalFileTrashDomain,
];

/// Services of the given types, grouped by type in the given order and
/// sorted by name within each type.
pub fn services_of<'a>(registry: &'a ServiceRegistry, types: &[ServiceType]) -> Vec<&'a Service> {
    let mut out = Vec::new();
    for t in types {
        let mut of_type: Vec<&Service> = registry.of_type(*t).map(AsRef::as_ref).collect();
        of_type.sort_by(|a, b| a.name.cmp(&b.name));
        out.extend(of_type);
    }
    out
}

fn colours(c: &RatingColours, incdec: bool) -> Json {
    let pair = |p: &hydrus_store::services::PenBrush| json!({"pen": p.pen.to_string(), "brush": p.brush.to_string()});
    if incdec {
        json!({"like": pair(&c.like), "mixed": pair(&c.mixed)})
    } else {
        json!({"like": pair(&c.like), "dislike": pair(&c.dislike), "null": pair(&c.null), "mixed": pair(&c.mixed)})
    }
}

/// The detailed description of one service, without its key.
pub fn describe(service: &Service) -> Map<String, Json> {
    let t = service.service_type();
    let mut d = Map::new();
    d.insert("name".into(), json!(service.name));
    d.insert("type".into(), json!(t.code()));
    d.insert("type_pretty".into(), json!(t.name()));
    let (display, shape) = match &service.kind {
        ServiceKind::RatingLike(c) => (Some(&c.display), Some(c.shape)),
        ServiceKind::RatingNumerical(c) => (Some(&c.display), Some(c.shape)),
        ServiceKind::RatingIncDec(display) => (Some(display), None),
        _ => (None, None),
    };
    if let Some(display) = display {
        d.insert("show_in_thumbnail".into(), json!(display.show_in_thumbnail));
        d.insert(
            "show_in_thumbnail_even_when_null".into(),
            json!(display.show_in_thumbnail_even_when_null),
        );
        if let Some(shape) = shape {
            let svg = match &service.kind {
                ServiceKind::RatingLike(c) => c.rating_svg.is_some(),
                ServiceKind::RatingNumerical(c) => c.rating_svg.is_some(),
                _ => false,
            };
            let label = if svg {
                "svg"
            } else {
                shape.name().unwrap_or("circle")
            };
            d.insert("star_shape".into(), json!(label));
        }
        d.insert(
            "colours".into(),
            colours(
                &display.colours,
                matches!(service.kind, ServiceKind::RatingIncDec(_)),
            ),
        );
    }
    if let ServiceKind::RatingNumerical(c) = &service.kind {
        d.insert("allows_zero".into(), json!(c.allow_zero));
        d.insert("min_stars".into(), json!(c.min_stars()));
        d.insert("max_stars".into(), json!(c.num_stars));
    }
    d
}

/// `{service_key_hex: description}` for every API-visible service.
pub fn services_dict(registry: &ServiceRegistry) -> Json {
    let mut out = Map::new();
    for service in services_of(registry, API_SERVICE_TYPES) {
        out.insert(service.key.to_hex(), Json::Object(describe(service)));
    }
    Json::Object(out)
}

/// A service's description including its key.
pub fn describe_with_key(service: &Service) -> Json {
    let mut d = describe(service);
    d.insert("service_key".into(), json!(service.key.to_hex()));
    Json::Object(d)
}

/// The same descriptions as a list, each with its key.
pub fn services_list(registry: &ServiceRegistry) -> Json {
    Json::Array(
        services_of(registry, API_SERVICE_TYPES)
            .into_iter()
            .map(describe_with_key)
            .collect(),
    )
}

/// The short form used in grouped listings.
pub fn brief(service: &Service) -> Json {
    json!({
        "name": service.name,
        "type": service.service_type().code(),
        "type_pretty": service.service_type().name(),
        "service_key": service.key.to_hex(),
    })
}
