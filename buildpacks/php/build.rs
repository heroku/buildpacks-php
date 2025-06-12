pub fn main() {
    // we want to re-compile anytime a test fixture changes
    // reason: an rstest macro generates test cases for each fixture in that directory
    println!("cargo::rerun-if-changed=tests/fixtures");
}
