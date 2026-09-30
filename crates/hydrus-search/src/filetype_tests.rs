//! The filetype tables, against the reference's.

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use hydrus_core::Mime;

    use crate::filetype::*;
    use crate::test_fixtures::fixture;

    #[test]
    fn filetype_words_match_reference_implementation() {
        let expected = fixture("system_predicates.json")["filetype_words"].clone();
        let ours: Vec<serde_json::Value> = FILETYPE_WORDS
            .iter()
            .map(|(word, mimes)| {
                let codes: Vec<u8> = mimes.iter().map(|m| m.code()).collect();
                serde_json::json!([word, codes])
            })
            .collect();
        assert_eq!(serde_json::Value::Array(ours), expected);
    }

    #[test]
    fn searchable_mimes_match_reference_implementation() {
        let constants = fixture("constants.json");
        let expected: BTreeSet<u8> = constants["mimes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["searchable"] == true)
            .map(|m| u8::try_from(m["id"].as_u64().unwrap()).unwrap())
            .collect();
        let ours: BTreeSet<u8> = SEARCHABLE_MIMES.iter().map(|m| m.code()).collect();
        assert_eq!(ours, expected);
        assert_eq!(ours.len(), SEARCHABLE_MIMES.len(), "no duplicates");
    }

    #[test]
    fn general_classes_match_reference_implementation() {
        let constants = fixture("constants.json");
        let groups = constants["general_mimetypes_to_mime_groups"]
            .as_object()
            .unwrap();
        let expected: BTreeSet<u8> = groups.keys().map(|k| k.parse().unwrap()).collect();
        let ours: BTreeSet<u8> = GENERAL_CLASSES.iter().map(|m| m.code()).collect();
        assert_eq!(ours, expected);
        for (class, members) in groups {
            let class = Mime::from_code(class.parse().unwrap()).unwrap();
            let expected: BTreeSet<u8> = members
                .as_array()
                .unwrap()
                .iter()
                .map(|m| u8::try_from(m.as_u64().unwrap()).unwrap())
                .collect();
            let ours: BTreeSet<u8> = Mime::members_of_class(class).map(Mime::code).collect();
            assert_eq!(ours, expected, "{class:?}");
        }
    }

    #[test]
    fn complete_classes_are_summarised() {
        let images: Vec<Mime> = Mime::members_of_class(Mime::GeneralImage)
            .filter(|m| is_searchable(*m))
            .collect();
        let set = FiletypeSet::new(images.iter().copied().chain([Mime::VideoMp4]));
        assert_eq!(
            set.summary().iter().copied().collect::<Vec<_>>(),
            vec![Mime::VideoMp4, Mime::GeneralImage]
        );
        assert_eq!(
            FiletypeSet::new([Mime::GeneralImage]).specific_mimes(),
            images.into_iter().collect()
        );
    }

    #[test]
    fn partial_classes_stay_specific() {
        let set = FiletypeSet::new([Mime::ImageJpeg, Mime::ImagePng]);
        assert_eq!(set.summary().len(), 2);
        assert_eq!(set.to_string(), "jpeg, png");
    }
}
