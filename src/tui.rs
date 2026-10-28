use std::{io, panic};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::Backend};

use crate::{
    event::{EventHandler, LlimEvent},
    llim_app::LlimApp,
    result::{LlimError, Result},
};

#[derive(Debug)]
pub struct Tui<B: Backend> {
    terminal: Terminal<B>,
    pub events: EventHandler,
}

impl<B: Backend> Tui<B> {
    pub fn new(terminal: Terminal<B>, events: EventHandler) -> Self {
        Self { terminal, events }
    }

    pub fn init(&mut self) -> Result<()> {
        terminal::enable_raw_mode().map_err(|e| LlimError::GeneralError(e.to_string().into()))?;
        crossterm::execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)
            .map_err(|e| LlimError::GeneralError(e.to_string().into()))?;

        let panic_hook = panic::take_hook();
        panic::set_hook(Box::new(move |panic_info| {
            Self::reset().expect("failed to reset the terminal");
            panic_hook(panic_info);
        }));

        self.terminal
            .hide_cursor()
            .map_err(|e| LlimError::GeneralError(e.to_string().into()))?;
        self.terminal
            .clear()
            .map_err(|e| LlimError::GeneralError(e.to_string().into()))?;
        Ok(())
    }

    pub fn draw(&mut self, app: &mut LlimApp, mut ui_effects: impl FnMut(&mut ratatui::Frame)) -> Result<()> {
        self.terminal
            .draw(|frame| ui_effects(frame))
            .map_err(|e| LlimError::GeneralError(e.to_string().into()))?;
        let _ = app;
        Ok(())
    }

    pub fn draw_frame(&mut self, mut render_fn: impl FnMut(&mut ratatui::Frame)) -> Result<()> {
        self.terminal
            .draw(|frame| render_fn(frame))
            .map_err(|e| LlimError::GeneralError(e.to_string().into()))?;
        Ok(())
    }

    fn reset() -> Result<()> {
        terminal::disable_raw_mode().map_err(|e| LlimError::GeneralError(e.to_string().into()))?;
        crossterm::execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)
            .map_err(|e| LlimError::GeneralError(e.to_string().into()))?;
        Ok(())
    }

    pub fn exit(&mut self) -> Result<()> {
        Self::reset()?;
        self.terminal
            .show_cursor()
            .map_err(|e| LlimError::GeneralError(e.to_string().into()))?;
        Ok(())
    }

    pub fn run_external_editor(&mut self, initial_text: &str) -> Result<Option<String>> {
        let tmp_path = std::env::temp_dir().join(format!("llim-edit-{}.md", std::process::id()));
        std::fs::write(&tmp_path, initial_text)
            .map_err(|e| LlimError::GeneralError(e.to_string().into()))?;

        Self::reset()?;
        let _ = self.terminal.show_cursor();

        let editor_raw = std::env::var("VISUAL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                std::env::var("EDITOR")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            })
            .unwrap_or_else(|| "nvim".to_string());

        let mut parts = editor_raw.split_whitespace();
        let primary_bin = parts.next().unwrap_or("nvim");
        let extra_args: Vec<&str> = parts.collect();

        let status = std::process::Command::new(primary_bin)
            .args(&extra_args)
            .arg(&tmp_path)
            .status()
            .or_else(|_| std::process::Command::new("vim").arg(&tmp_path).status())
            .or_else(|_| std::process::Command::new("nano").arg(&tmp_path).status())
            .or_else(|_| std::process::Command::new("vi").arg(&tmp_path).status());

        terminal::enable_raw_mode()
            .map_err(|e| LlimError::GeneralError(e.to_string().into()))?;
        crossterm::execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)
            .map_err(|e| LlimError::GeneralError(e.to_string().into()))?;
        let _ = self.terminal.hide_cursor();
        let _ = self.terminal.clear();

        while self.events.try_next().is_some() {}

        let edited = match status {
            Ok(s) if s.success() => std::fs::read_to_string(&tmp_path).ok(),
            _ => None,
        };
        let _ = std::fs::remove_file(&tmp_path);
        Ok(edited)
    }
}

#[allow(dead_code)]
const _: fn(LlimEvent) = |_| {};
