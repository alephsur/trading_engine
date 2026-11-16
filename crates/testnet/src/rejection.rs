//! Expected pre-dispatch rejection; never used for transport or account uncertainty.
#[derive(Debug)]
pub struct Rejection {
    pub code: &'static str,
    message: &'static str,
}
impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message)
    }
}
impl std::error::Error for Rejection {}
pub fn check(condition: bool, code: &'static str, message: &'static str) -> anyhow::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Rejection { code, message }.into())
    }
}
