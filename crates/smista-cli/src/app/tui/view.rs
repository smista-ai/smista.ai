pub(in crate::app::tui) mod console;

use super::Tui;
use crate::app::tui::state::ActiveComponentState;

impl<B> Tui<B>
where
    B: ratatui::backend::Backend,
{
    pub fn view(&mut self) -> anyhow::Result<()> {
        self.render_view(true)
    }

    pub(in crate::app::tui) fn render_view(&mut self, sync_history: bool) -> anyhow::Result<()> {
        tracing::debug!("rendering TUI view");

        if sync_history && self.insert_transcript_entries()? {
            self.pin_inline_viewport_to_bottom();
        }

        self.terminal
            .draw(|frame| match &self.state.active_component {
                ActiveComponentState::Console(console_state) => {
                    console::view_console(
                        frame,
                        console_state,
                        &self.state.history,
                        self.state.execution_turn.as_ref(),
                        self.state.preferred_model.as_ref(),
                        self.state.router,
                        &self.context.cwd,
                    );
                }
                ActiveComponentState::SkillList(_list_state) => todo!(),
                ActiveComponentState::ModelsList(_list_state) => todo!(),
                ActiveComponentState::ProvidersList(_list_state) => todo!(),
                ActiveComponentState::Usage(_usage_state) => todo!(),
                ActiveComponentState::TracingList(_list_state) => todo!(),
                ActiveComponentState::SessionsList(_list_state) => todo!(),
            })
            .map_err(|err| anyhow::anyhow!("failed to render TUI view: {err}"))?;

        Ok(())
    }
}
