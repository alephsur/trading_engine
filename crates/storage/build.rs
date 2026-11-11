fn main() {
    // sqlx::migrate! embeds SQL at compile time; newly added files must rebuild it.
    println!("cargo:rerun-if-changed=../../migrations");
}
