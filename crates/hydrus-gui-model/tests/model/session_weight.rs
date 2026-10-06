//! Pages > weight > "total session weight"'s information, as
//! `_ShowPageWeightInfo` words it.
use hydrus_gui_model::session_weight::report;

#[test]
fn the_report_weighs_files_as_one_and_urls_as_twenty() {
    let text = report(3, (1_500, 100), 1, (10, 2));
    assert!(text.starts_with("Session weight is a simple representation"));
    assert!(text.contains("Your 3 open pages' total is: 3,500\n\n"));
    assert!(text.contains("your file weight is 1,500 and URL weight is 2,000."));
    assert!(text.ends_with(
        "your 1 closed pages (in the undo list) have total weight 50, being file weight 10 and URL weight 40."
    ));
}
