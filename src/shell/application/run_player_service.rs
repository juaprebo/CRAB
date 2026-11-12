use crate::shell::domain::player_runtime::PlayerRuntime;
use anyhow::Result;

pub struct RunPlayerService;
impl RunPlayerService {
    pub fn execute(&self, runtime: &mut impl PlayerRuntime) -> Result<()> {
        while !runtime.should_quit() {
            runtime.frame()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Runtime {
        frames: usize,
        fail: bool,
    }
    impl PlayerRuntime for Runtime {
        fn should_quit(&self) -> bool {
            self.frames == 2
        }
        fn frame(&mut self) -> Result<()> {
            anyhow::ensure!(!self.fail, "terminal unavailable");
            self.frames += 1;
            Ok(())
        }
    }
    #[test]
    fn stops_on_quit_and_returns_runtime_failures_without_another_frame() {
        let mut runtime = Runtime {
            frames: 0,
            fail: false,
        };
        RunPlayerService.execute(&mut runtime).unwrap();
        assert_eq!(runtime.frames, 2);
        let mut failed = Runtime {
            frames: 0,
            fail: true,
        };
        assert!(RunPlayerService.execute(&mut failed).is_err());
        assert_eq!(failed.frames, 0);
    }
}
