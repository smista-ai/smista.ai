pub(in crate::app::tui) mod console;
mod select;

use super::Tui;
use crate::app::tui::state::ActiveComponentState;

impl<B> Tui<B>
where
    B: super::ClearableBackend,
{
    pub fn view(&mut self) -> anyhow::Result<()> {
        self.render_view(true)
    }

    pub(in crate::app::tui) fn render_view(&mut self, sync_history: bool) -> anyhow::Result<()> {
        tracing::debug!("rendering TUI view");

        self.try_pin_inline_viewport_to_bottom()?;
        if sync_history {
            self.insert_transcript_entries()?;
        }
        if let ActiveComponentState::LogsList(logs_list) = &mut self.state.active_component {
            logs_list.replace_entries(self.context.logs.entries());
        }

        self.terminal
            .draw(|frame| match &self.state.active_component {
                ActiveComponentState::Console(console_state) => {
                    console::view_console(
                        frame,
                        console_state,
                        self.state.execution_turn.as_ref(),
                        self.state.preferred_model(),
                        self.state.router,
                        &self.context.cwd,
                        self.state.plan,
                    );
                }
                ActiveComponentState::LogsList(logs_list) => {
                    select::view_select(frame, "Logs", "No logs", logs_list, select::log_line);
                }
                ActiveComponentState::SkillList(list_state) => {
                    select::view_select(
                        frame,
                        "Skills",
                        "No skills found",
                        list_state,
                        select::skill_line,
                    );
                }
                ActiveComponentState::ModelsList(list_state) => {
                    select::view_select(
                        frame,
                        "Models",
                        "No models found",
                        list_state,
                        select::model_line,
                    );
                }
                ActiveComponentState::ProvidersList(list_state) => {
                    select::view_select(
                        frame,
                        "Providers",
                        "No providers found",
                        list_state,
                        select::provider_line,
                    );
                }
                ActiveComponentState::Usage(_usage_state) => todo!(),
                ActiveComponentState::TracingList(list_state) => {
                    select::view_select(
                        frame,
                        "Trace",
                        "No trace events",
                        list_state,
                        select::trace_line,
                    );
                }
                ActiveComponentState::SessionsList(list_state) => {
                    select::view_select(
                        frame,
                        "Resume Session",
                        "No sessions found",
                        list_state,
                        select::session_line,
                    );
                }
            })
            .map_err(|err| anyhow::anyhow!("failed to render TUI view: {err}"))?;

        Ok(())
    }
}
