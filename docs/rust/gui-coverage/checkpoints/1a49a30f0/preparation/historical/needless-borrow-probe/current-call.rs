fn capture_saved_navigation_options(_labels: &[u32]) {}
fn main() {
    let fixture = vec![1_u32, 2, 3];
    let labels: &Vec<u32> = &fixture;
    capture_saved_navigation_options(&labels);
}
