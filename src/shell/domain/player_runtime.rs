use anyhow::Result;

pub trait PlayerRuntime {
    fn should_quit(&self) -> bool;
    fn frame(&mut self) -> Result<()>;
}
