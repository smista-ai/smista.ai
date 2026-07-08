use super::Tui;

impl<B> Tui<B>
where
    B: ratatui::backend::Backend,
{
    pub(in crate::app::tui) fn view(&mut self) -> anyhow::Result<()> {
        tracing::debug!("rendering TUI view");

        self.terminal
            .draw(|_frame| {})
            .map_err(|err| anyhow::anyhow!("failed to render TUI view: {err}"))?;

        Ok(())
    }
}
