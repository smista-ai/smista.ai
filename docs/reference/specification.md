# Introduction

## Vision

Smista.ai is a local-first agent and CLI that routes each phase of an AI
workflow to the most suitable model using deterministic, configurable
policies.

The core functionality is to route a task to the most suitable LLM model
based on the user\'s intent.

The goal is not to clone Claude Code, Codex, or similar tools. Instead,
smista.ai aims to provide the core primitives expected from a modern
coding/text agent:

- Prompt templates

- Plan mode

- Skills

- Tool permissions

- Context management

- Diff and review

- Traceability

Smista.ai's main differentiator is deterministic multi-model routing.

## Problem

In 2026, most users neither use a single LLM model (e.g. Claude Opus,
Claude Sommnet) nor a single provider (e.g. OpenAI, Anthropic, ...).

This raises a common problem: for users is annoying and complicated to
switch context:

- Users need to switch apps or CLIs

- Users need to copy the context around

- Users need to remember which model to use

- A single task may require multiple model switches

- It's hard to integrate local models to cut costs.

## Solution

Smista.ai wants to provide two main components: smista-cli, a CLI tool
like Claude code, where the user asks the model to perform tasks, and
instrada-router, a service which exposes an API, deterministically
detects the user's intent, and forwards the request to the most suitable
provider/model LLM pair. The response is then returned to the user.

### Product surfaces

In the first (V1) version, the CLI and the service will both run on
localhost.

In the future we aim to include the possibility of having a SaaS to run
instrada-router.

### Core idea

Users configure how work should be routed.

For example:

- Planning: strongest reasoning model

- Simple edits: local model

- Core review: reliable coding model

- Summarisation: cheap/local model

- Sensitive files: local-only model

- A certain file path: only Codex

Smista.ai applies these rules consistently and explains every routing
decision.

### Golden workflow

A user should be able to run:

smista "refactor the auth middleware"

smista then shows:

- Detected task: edit

- Selected model: claude-sonnet

- Matched rule: edits under src/auth/\*\* use claude-sonnet

- Included context: src/auth/middleware.rs, current git diff, SMISTA.md

- Excluded context: .env, secrets.toml

- Estimated cost: \$0.08-\$0.14

- Required permissions: file write approval

Before any write, smista presents a diff and asks for confirmation.

After execution, the user can run:

smista trace or /trace

and see the full routing decision, context selection, tool calls,
approvals, and cost.

The user must also be able to have a preview of the routing:

smista route "review this PR" or /route "review this PR"

and have as output:

Task: review

Selected model: gpt-5.5-thinking

Reason: matched rule \`task.review -\> openai/gpt-5.5-thinking\`

Context:

included:

\- current git diff

\- SMISTA.md

\- src/auth/\*\*

excluded:

\- .env

\- target/\*\*

\- secrets/\*\*

Estimated cost: \$0.03-\$0.09

Will require:

\- read repository: allowed

\- write files: ask

\- shell commands: ask

### Local-first model

Smista.ai can work without a SaaS dependency.

Configuration, project instructions, skills, prompts, routing rules, and
traces live locally and can be versioned per project.

Remote provides are supported, but local models are first-class.

### Agent primitives

Smista.ai provides the essential primitives expected from modern agent
workflows:

- Project instructions

- Prompt templates

- Plan mode

- Skills

- Tool permissions

- Context management

- Diff review

- Execution trace

### Provider model

Smista.ai treats each model source as a provider.

Initial providers:

- Ollama for local models

- OpenAI API

- Anthropic API

- OpenAI-compatible endpoints

Future providers may include Gemini and others.

Ollama is used as a local model backend, not as the main router.

### Authentication model

Smista.ai supports official and stable authentication methods for the
models:

- API keys

- Environment variables

- OS keychains

- Credential helpers

- Local endpoints

Smista.ai does not rely on unofficial subscription access.

### Future SaaS

A SaaS version may later provide account management, license management,
config sync, usage dashboard, team policies, and setup for local models.

## Audience

Smista.ai is useful for developers who work with multiple AI models and
want more control over when, why, and how each model is used.

In principle, the audience includes any developer using AI-assisted
workflows. In practice, the initial audience is narrower because current
provider authentication models make consumer subscription-based usage
difficult or unavailable for third-party tools.

### Power users and technical teams

Smista.ai is most useful for developers and teams who already use
multiple models, local runtimes, or API-based providers.

These users care about:

- Choosing the right model for each task

- Reducing manual model switching

- Using local models for simple or sensitive work

- Controlling API costs by preventing the usage of expensive models for
  simple tasks

- Making model usage explainable

- Versioning project-level AI configuration

- Avoiding accidental use of expensive models

- Applying common policies for model usage in their teams.

This includes:

- Senior developers

- AI-heavy engineering teams

- Open-source maintainers

- Teams experimenting with local models

- Companies with internal AI policies

- Teams already paying for API usage

### Enterprise audience

Smista.ai may be especially valuable for companies, because it can
provide governance around model usage.

For enterprises, the value is less about convenience and more about
control:

- Enforce which models can be used

- Keep sensitive files local

- Route low-risk tasks to cheaper models

- Reserve frontier models for high-value tasks

- Audit which model was used and why

- Control budget per project, team, or task type

- Standardise AI usage across repositories

- Avoid uncontrolled usage of random tools and providers.

In this context, smista.ai becomes an AI workflow control layer rather
than just a developer convenience.

# Product Specifications

## Product Principles

Smista.ai should be designed around a few strong product principles.

### Local-first by default

Smista.ai must be able to work without a SaaS dependency.

Project configuration, prompts, skills, routing rules, traces, and
instructions should be stored locally and versioned when useful.

Remote services will be available later, but the local CLI must remain
useful on its own.

### Deterministic over magical

Smista.ai should not behave like a black box and should never rely on
LLMs for decision-making.

Explicit routing rules and project policies must guide model selection.
When ambiguous, the user must be able to provide a decision about the
model to use.

### Traceability over routing decision

If smista.ai chooses a model, the user must be able to ask why.

A trace should show:

- Selected model

- Matched rule

- Fallback used, if any

- Task classification

- Context included

- Tools executed

- Estimated cost, if possible

### Local when possible, frontier when needed

Smista.ai should make it easy to use cheaper local models for simple or
sensitive tasks, and reserve stronger, more advanced providers when
needed.

This principle supports both cost control and privacy

### User control before automation

Smista.ai must provide a prompt to the user for approval on:

- File writes

- Shell commands

- Network access

- Sending sensitive files to remote providers

### Official integrations only

Smista.ai should only use official, supported and ToS-compliant
authentication methods for LLM providers.

### Project policies should be versionable

AI behaviour should be part of the project, not just a hidden user
state.

Teams should be able to review and version:

- Routing rules

- Project instructions

- Allowed tools

- Privacy constraints

- Skills

- Prompt templates

### One workflow, many models

The user should not need to switch between CLIs, web apps, and providers
constantly.

Smista.ai should provide one coherent workflow while still allowing
different models to handle different phases.

### Safe defaults, configurable power

The default experience should be safe and understandable.

Advanced users should be able to opt into:

- Auto-apply

- Custom routing rules

- Local-only mode

- Model fallback chains

- Project-specific skills

- Cost budgets

### Do not hide costs

When using paid APIs, smista.ai should, as far as possible, make the
cost visible.

The user should be able to understand and control model usage before it
becomes too expensive for them.

### User is always in control

Smista.ai should automate routing, but never remove user agency.\
\
The user must always be able to explicitly delegate a specific task to a
specific model, even if that choice overrides the current routing
policy.\
\
Project and organisation policies may define defaults, warnings, or
restrictions, but the user should always understand when a policy is
being applied and when an override is possible.

### Least-context routing

Smista.ai should treat context as something to intentionally select, not
something to blindly forward.\
When a workflow is split across multiple models, each model should
receive only the minimum context required for its assigned task.\
Full session or project context should never be passed by default.

## Actors

- Developer: the primary user of smista.ai. A developer uses smista.ai
  from the CLI to run prompts, plan changes, edit code, review diffs,
  execute skills, and inspect routing decisions. The developer may work
  alone or inside a team-managed project.

- Maintainer: The maintainer is responsible for configuring and defining
  policies for smista.ai. It can be the same developer, but in an
  enterprise context, it is probably a separate person.

- Model provider: An external or local system that executes model
  requests (e.g. Ollama, OpenAI, ...).

## Use Cases

### UC1 -- Run an interactive session

As a Developer, I want to start an interactive smista.ai session, so
that I can execute prompts through the configured routing policy.

#### Preconditions

- Smista.ai is installed

- At least one provider is configured

- A routing policy exists

- The user is signed in

#### Main flow

1.  The developer starts smista.ai

2.  Smista.ai loads the merged configuration (global + local)

3.  Smista.ai loads project instructions, prompts, skills, and tool
    permissions.

4.  Smista.ai signs in.

5.  The developer submits a prompt.

6.  Smista.ai loads the user's session from the storage.

7.  Smista.ai classifies the task

8.  Smista.ai resolves the required skills to execute the task.

9.  Smista.ai selects a model according to the active routing policy.

10. Smista.ai builds the context the model needs to execute the task.

11. Smista.ai forwards the payload to the chosen model.

12. The model provider returns either a direct response or a tool
    request.

13. If the provider returns a direct response, smista.ai displays it to
    the developer.

14. Smista.ai saves the updated session to the storage.

15. If the provider requests a tool call, smista.ai validates it against
    the active permissions.

16. If approval is required, smista.ai asks the developer to approve or
    reject the tool call.

17. If the tool call is allowed, smista.ai executes it through the tool
    runtime.

18. Smista.ai sends the tool result back to the selected model if
    needed.

19. The model continues until it returns a final response or no further
    tool calls are required.

20. Smista.ai displays the response.

21. Smista.ai records the routing decision, context selection, tool
    calls, approvals, and provider interactions in the trace.

#### Alternative flows

- If no routing rule matches, smista.ai uses the configured default
  model.

- If the selected provider is unavailable, smista.ai uses a configured
  fallback model.

- If the context contains restricted files, smista.ai excludes them or
  asks for confirmation.

- If the requested tool is denied by policy, smista.ai blocks the tool
  call and reports the reason.

- If the developer rejects a tool call, smista.ai does not execute it
  and informs the model of the rejection.

- If a tool call fails, smista.ai reports the failure and may allow the
  model to recover or ask the developer what to do next.

- If the model requests a write operation, smista.ai shows the proposed
  change or diff before applying it, unless auto-apply is explicitly
  enabled.

- If the Developer explicitly overrides the model, smista.ai uses the
  requested model only if allowed by policy.

### UC2 -- Execute a plan-first workflow

As a developer, I want to create a plan before modifying files, so that
I can review the intended approach before smista.ai performs any
changes.

#### Preconditions

- Smista.ai is configured

- The project is accessible

- A model is configured for planning tasks

#### Main flow

1.  The developer requests a plan for an implementation.

2.  Smista.ai loads the workspace configuration.

3.  Smista.ai loads relevant project instructions

4.  Smista.ai selects the minimum required context.

5.  Smista.ai classifies the task as a plan.

6.  Smista.ai selects the planning model.

7.  Smista.ai sends the task and selected context to the provider.

8.  The model provider returns a proposed plan.

9.  Smista.ai displays the plan.

10. If the destination is not already authorised, smista.ai asks the
    user whether it can write the plan at the destination designed in
    the configuration.

11. Smista.ai stores the plan in the current session.

#### Alternative flows

- If the developer explicitly selects another model, smista.ai users
  will be allowed by policy.

- If the selected context exceeds the model's limit, smista.ai reduces
  or summarises it.

- If the developer rejects the plan, smista.ai discards it.

### UC3 -- Configure project routing policy

As a Maintainer, I want to define project-level routing rules, so that
all developers working on the project share consistent model usage,
privacy constraints, and tool permissions.

#### Preconditions

- Smista.ai is installed

- The maintainer has write access to the repository.

#### Main Flow

1.  The maintainer creates or edits \`.smista/config.toml\`

2.  The maintainer defines project routing rules.

3.  The maintainer defines allowed providers and models (Authentication,
    location, parameters).

4.  The maintainer defines privacy constraints.

5.  The maintainer defines tool permissions.

6.  The maintainer commits the configuration to the repository.

7.  A developer runs smista.ai in the project

8.  Smista.ai loads the project configuration by merging local values
    into the global values from the user's home.

9.  Smista.ai applies the project routing policy.

#### Alternative flows

- If the project configuration is invalid, smista.ai reports validation
  errors on run.

- If the developer has a conflicting global config, smista.ai applies
  the documented precedence rules (local wins).

### UC4 -- Override model selection

As a developer, I want to explicitly choose the model for a specific
task, so that I can override default routing when needed.

#### Preconditions

- The requested model is configured

- The active policy allows user overrides.

- Smista.ai is installed.

#### Main flow

1.  The developer submits a task with an explicit model.

2.  Smista.ai validates the requested model

3.  Smista.ai checks whether the override is allowed by the
    configuration

4.  Continues with UC1.7

#### Alternative flows

- If the specified model doesn't exist, the system returns an error to
  the user.

### UC5 -- Inspect routing trace

As a developer, I want to inspect the routing trace, so that I can
understand which model was used, why it was selected, what context was
sent, and which tools were executed.

#### Preconditions

- At least one smista.ai operation has been executed

#### Main flow

1.  The developers request the trace

2.  Smista.ai loads the latest operation trace

3.  Smista.ai displays the selected model

4.  Smista.ai displays the matched routing rule

5.  Smista.ai displays any fallbacks or overrides

6.  Smista.ai displays included context.

7.  Smista.ai displays executed tools.

8.  Smista.ai displays the estimated cost if available.

#### Alternative flows

- If no trace is available, smista.ai reports that no previous operation
  exists.

### UC6 -- Session storage

As a developer, I want to resume a previous session to continue working
in the existing context.

#### Preconditions

- A previous session must exist

#### Main flow

1.  The developer asks smista.ai to retrieve existing sessions.

2.  Smista.ai returns the user\'s existing sessions.

3.  The developer tells smista.ai to resume a certain session

4.  Smista.ai loads context from session storage.

5.  Smista.ai returns to the user the history of executed tasks and
    responses.

#### Alternative flows

- If the requested session doesn't exist, smista.ai returns an error

### UC7 -- Preview the route for a task

As a developer, I want to have a preview for a task before executing it.

#### Main flow

1.  The developer starts smista.ai

2.  Smista.ai loads the merged configuration (global + local)

3.  Smista.ai loads project instructions, prompts, skills, and tool
    permissions.

4.  Smista.ai signs in.

5.  The developer calls \`/route "task..."\`

6.  Smista.ai loads the user's session from the storage.

7.  Smista.ai classifies the task

8.  Smista.ai resolves the required skills to execute the task.

9.  Smista.ai selects a model according to the active routing policy.

10. Smista.ai returns a payload containing the trace for the routing
    that would be used to execute the command.

11. Smista-cli displays the trace to the user.

## Requirements

### Functional requirements

#### CLI

- Smista.ai shall provide a command-line interface (exec called
  \`smista\`, package \`**smista-cli**\`)

- Smista-cli shall support interactive sessions.

- Smista-cli shall support one-shot commands.

- Smista-cli shall support project initialisation through \`init\`.

- Smista-cli shall support explicit task commands, including plan, edit,
  review, prompt, skill, and trace.

- Smista-cli must provide commands to get current usage and costs for
  models.

- Smista-cli must provide auto-complete for paths prefixed with \`@\`.

- Smista-cli must provide a command for resuming sessions \`/resume
  \<session-id\>\`.

- Smista-cli commands are executed by prepending a \`/\` to the command.

- Smista-cli shall display streaming model output when supported by the
  selected provider.

- Smista-cli shall provide clear errors when configuration, providers,
  models, or credentials are invalid.

#### Routing

- Smista shall route tasks to models using deterministic routing
  policies

- Routing must happen on \`**smista-router**\`.

- Smista-router shall support routing by skill.

- Smista-router shall support routing by file path.

- Smista-router shall support routing by intent.

- Smista-router shall support fallback models when the selected model or
  provider is unavailable.

- Smista-router shall support session storage for each developer's
  session.

- Smista-router shall support authentication to allow developers to
  access their session storage.

- Smista-router shall allow developers to select the models for
  executing a task.

- Smista-router shall explain which model was selected and why.

- Smista-router shall record routing decisions in the execution trace.

- Smista-router shall support interfacing with LLM providers and
  different models.

### Task classification

- Smista-router shall classify user requests into supported task types.

- Smista-router shall support at least the following task types:

  - Chat

  - Plan

  - Edit

  - Review

  - Summarize

  - Prompt

  - Skill

- Smista-router shall allow explicit commands to override automatic task
  classification.

- Smista-router shall expose the detected task type in the trace

- Smista-router shall deterministically classify tasks.

- Smista-router shall be able to classify tasks by:

  - File path

  - User's intent

  - Involved skills

### Context management

- smista-router shall select only the minimum context required for the
  current task.

<!-- -->

- smista-router shall not forward the full session, project, or
  conversation context by default.

- smista-router shall include project instructions when relevant.

- smista-cli shall include explicitly referenced files when allowed by
  policy.

- smista-cli shall include the current git diff when relevant.

- smista-cli shall respect \`.gitignore\`.

- smista-cli shall respect \`.smistaignore\`.

- smista-cli shall support explicit path deny rules.

- smista-cli shall prevent the sending of restricted files to remote
  providers.

- smista-router shall record which context was sent to the selected
  provider.

- smista-router shall adapt the selected context to the selected
  model\'s context limits.

- Smista-router shall save the current session's context into the
  session storage, and update it after every call.

### Project instructions

- smista-cli shall support project-level instructions.

- Smista-cli shall load project instructions from a documented location.

- Smista-cli shall support SMISTA.md or .instrada/SMISTA.md.
  \`.instrada/SMISTA.md\` has priority.

- Smista-cli shall define clear precedence between system defaults,
  global instructions, and project instructions. **The general rule is
  that local overrides global; globals are kept if local doesn't
  override them.**

- Smista-cli shall record loaded instructions in the trace.

### Prompt templates

- Smista-cli shall support reusable prompt templates

- Smista-cli shall support global prompt templates

- Smista-cli shall support project-level prompt templates

- Smista-cli shall allow prompt templates to consume variables such as:

  - Current diff

  - Selected files

  - Branch name

  - Stdin

  - User-provided arguments

- Smista-cli shall allow prompt templates to define optional model
  preferences

- Smista-cli shall report an error when a requested prompt template does
  not exist.

### Skills

- Smista-cli shall support reusable skills.

- Smista-cli shall support global skills. Global skills must be read
  from the default path \`\~/.agents/skills\`

- Smista-cli shall support project-level skills at \`.smista/skills\`

- Skills must follow the standard format:

  - SKILL.md file defines the behavior of the skill.

  - Any file related to the skill should be located in the same folder
    of SKILL.md

- A skill shall be able to define:

  - name

  - description

  - instructions

  - preferred model

  - fallback model

  - allowed tools

  - execution mode

- smista-cli shall allow users to run a skill explicitly.

- Smista-cli may support automatic skill selection.

- Smista-cli shall apply skill-specific routing preferences when a skill
  is executed.

- Smista-cli shall apply skill-specific tool permissions when a skill is
  executed.

- Smista-cli shall report an error when a requested skill does not
  exist.

- Smista-cli shall record skill execution in the trace.

### Plan mode

- smista shall support a plan mode.

- Plan mode shall not modify files by default.

- Plan mode shall produce a structured execution plan.

- Smista-cli shall allow the user to approve, reject, or revise a plan.

- Smista-cli shall store accepted plans in the current session at the
  user-defined directory; default \`.smista/plans/...\`

- Smista-router shall route plan tasks according to the active routing
  policy.

- Smista-router shall record plan creation and routing decisions in the
  trace.

### Tool mediation

- smista shall mediate all model-requested tool calls.

- Models shall not directly access the file system, shell, network, or
  project state.

- Smista-router shall validate each tool call against the active tool
  permissions.

- Smista shall support tool permission modes:

  - \`allow\`

  - \`ask\`

  - \`deny\`

- Smista-router shall ask for user approval when a tool call requires
  confirmation.

- Smista-router shall block denied tool calls.

- Smista-router shall report blocked tool calls to the user.

- Smista-router shall return tool results to the model when required.

- Smista-router shall record tool calls, approvals, rejections, and
  results in the trace.

### File editing and diff review

- smista shall propose file changes as diffs by default.

- smista shall not silently write files by default.

- smista shall ask the user to approve file modifications unless
  auto-apply is explicitly enabled and allowed by policy.

- smista shall validate patches before applying them.

- smista shall not modify files when a patch cannot be applied cleanly.

- smista shall allow the user to reject proposed changes.

- smista shall allow the user to request revisions to proposed changes.

- smista shall record applied and rejected patches in the trace.

### Provider support

- smista shall support local model providers through Ollama.

- smista shall support remote model providers. Initially, OpenAI and
  Claude.

- smista shall support OpenAI-compatible endpoints.

- smista shall expose provider capabilities such as:

  - streaming support

  - tool support

  - context window

  - model name

  - , local or remote execution

  - costs monitoring

- Interaction with remote models must happen through \`rig\`.

- Smista shall report an error when a requested provider or model is
  unavailable.

- Smista shall not treat Ollama as the main router; Ollama shall be used
  as a local model backend.

### Model authentication

- smista-cli shall support API keys through environment variables.

- Smista-cli shall support API keys through local configuration.

- Smista-cli shall support secrets through \`.smista/secrets\` file,
  injectable inside of configuration by key name.

- Smista-cli shall support OS keychain storage where available.

- Smista-cli shall support credential helpers.

- Smista-cli shall support unauthenticated local endpoints when
  appropriate.

- Smista shall support OpenAI-compatible endpoints with optional API
  keys.

- Smista-router shall not depend on unofficial subscription login flows.

- Smista-router shall not depend on browser scraping, cookies, or reused
  Claude Code/Codex sessions.

- Smista-router shall clearly report missing or invalid credentials.

### User authentication

- Smista-router shall authenticate users before accessing user-scoped
  resources.

- Smista-cli shall support authentication against the router service.

- A persistent user record in the database shall represent each
  authenticated user.

- Each session shall be associated with the authenticated user who
  created it.

- Users shall only be able to access their own sessions.

- Users shall only be able to resume, inspect, delete, or archive
  sessions they own.

- Smista shall support API-key-based authentication for the router
  service.

- Api keys may be generated locally for local-first usage

- Api keys may be generated by the SaaS control plane in future
  versions.

- Api keys shall be stored securely on the client side, preferably using
  the OS keychain when available.

- Api keys shall not be logged, displayed in traces, or exposed in error
  messages.

- The router service shall validate Api keys before processing
  authenticated requests

- The router service shall reject missing, invalid, expired, or revoked
  Api keys.

- Smista-router must expose an endpoint for getting a new API key and
  the user ID. This token must be given without any requirement.

- Smista-router must expose and endpoint for signing in with the user ID
  and the API key. A token is returned to authenticate the session and
  must be dropped after the session ends. The token must be used for
  each interaction with smista-router after.

- A proxy shall be provided in front of the smista-router if the
  smista-router owner wants to protect the API key provider.

- The API key must be 96 alphanumeric long, prefixed by
  \`sk-smista-apiXX-\`, where \`XX\` is version (i.e. 01).

### Session

- Smista-router shall persist session state.

- Each session shall be associated with a single authenticated user.

- Each session shall have a unique UUIDv7 identifier.

- A session shall store, at a minimum:

  - Session id

  - User id

  - Creation timestamp

  - Last update timestamp

  - Messages

  - Selected models

  - Routing decisions

  - Context references

  - Tool calls

  - Approvals and rejections

  - Generated plans

  - Proposed and applied diffs

  - Execution trace metadata

- Smista shall allow users to resume a previous session.

- Smista shall support a \`/resume \<session-id\>\` command inside the
  CLI.

- Resuming a session shall restore the previous conversation and
  relevant execution state.

- Smista-router shall prevent other users from accessing sessions owned
  by the current user.

- Smista-router shall support deleting or archiving previous sessions.

- Smista-router should separate session metadata from session content to
  support future end-to-end encryption.

### Storage backend

- Smista-router shall support a storage layer for users, sessions,
  tokens, etc.

- The storage layer should support both local embedded deployments and
  scalable server-side deployments. For this reason, the best fit for
  this case is to use SurrealDB.

### Configuration

- Smista shall support global configuration. The default directory must
  be \~/.config/smista, with the default configuration file at
  \~/.config/smista/config.toml. On Windows systems,
  \~/.smista/config.toml.

- smista shall support project-level configuration at
  .smista/config.toml.

- Project-level configuration shall override global configuration
  according to documented precedence rules.

- Smista shall support versionable project configuration.

- Smista shall validate configuration files.

- Smista shall report invalid configuration with actionable errors.

- Smista shall support configuration for:

  - providers

  - models

  - routing

  - fallbacks

  - tool permissions

  - privacy constraints

  - skills

  - prompt templates

### Privacy and security

- must support local-only mode.

- Smista shall support no-network mode.

- Smista shall support read-only mode.

- Smista shall prevent configured restricted paths from being sent to
  remote providers.

- Smista shall warn or block when sensitive files are detected, in
  accordance with policy.

- smista shall not enable telemetry by default.

- smista shall avoid logging secrets.

- Smista shall avoid exposing API keys in traces, logs, or errors.

- Smista shall record security-relevant decisions in the trace.

### Traceability

- smista shall record an execution trace for each task.

- The trace shall include:

  - selected model

  - matched routing rule

  - task type

  - provider used

  - fallbacks used

  - explicit model overrides

  - selected context

  - excluded restricted context

  - tool calls

  - approvals and rejections

  - estimated and actual costs when available

- Smista shall provide a command to inspect the latest trace.

- Smista shall provide a command to explain why a model was selected.

### Usage and cost visibility

- Smista shall estimate token usage when possible.

- Smista shall estimate the cost when provider pricing is available.

- Smista shall disclose usage information for each task.

- Smista should expose usage information per project.

- Smista should support budget limits in a future version.

- smista shall make expensive model usage visible to the user.

## Non-functional requirements

### Reliability

- Smista shall handle provider failures gracefully.

- Smista shall support retry behaviour where appropriate.

- Smista shall support provider fallback.

- Smista shall provide clear error messages.

- Smista shall avoid partial file writes.

- Smista shall preserve user work in the event of an operation failure.

### Performance

- Smista should start quickly.

- Smista should stream output when supported.

- Smista should avoid sending unnecessary context.

- Smista should cache reusable context where appropriate.

- Smista should avoid unnecessarily recomputing expensive project
  metadata.

### Portability

- Smista should support macOS, Windows, and Linux in V1.

- Smista may support Windows in a later version.

- Smista should be distributable as a single binary where practical.

### Maintainability

- Smista should use a modular architecture.

- Provider integrations should be isolated behind a provider
  abstraction.

- Routing logic should be isolated from provider implementations.

- Configuration parsing and validation should be centralised.

- Tool execution should be isolated behind a tool mediation layer.

### Extensibility

- Smista should allow new providers to be added without changing core
  routing logic.

- Smista should allow new skills to be added without changing core code.

- Smista should allow new prompt templates to be added locally.

- Smista should be designed to support future SaaS/control-plane
  features without requiring them.

### Usability

- Smista should provide safe defaults.

- Smista should make common workflows easy to run.

- Smista should avoid requiring users to understand all routing rules
  before first use.

- Smista should provide actionable errors and suggestions.

- Smista should make model overrides explicit and easy.

## Technical constraints

- Smista should be implemented primarily in Rust

- Smista should expose its core runtime as reusable internal crates.

- Smista should support native CLI distribution.

- Smista should avoid requiring a runtime for core usage.

# Technical Specifications

## System components

Smista.ai is composed of a small set of clearly separated system
components.

The goal of this architecture is to keep the CLI user experience simple
while providing an easy-to-set-up router as the CLI\'s backend.

The main system components are:

- Smista-cli

- Smista-router

- Smista-core

- Smista-SDK

### Smista-cli

Smista-cli is the command-line interface for developers.

It is responsible for user interaction, command parsing, terminal
rendering, local workspace discovery, and approval prompts.

The CLI must support both interactive sessions and one-shot commands.
Mind that one-shot commands also trigger the interaction, and work as a
shortcut to start it.

The CLI is responsible for:

- Starting and resuming user sessions

- Reading project-local files when required

- Displaying streaming model output

- Displaying routing preview

- Showing diffs before writes

- Asking the user for approvals

- Displaying traces, costs, warnings, and errors

- Communicating with smista-router.

The CLI must not directly decide which model should execute a task.
Model selection belongs to the router. The developer, though, can
specify a preference using the CLI.

### Smista-router

Smista-router is the routing and orchestration service.

It receives requests from smista-cli, through a REST JSON API,
authenticates the user, loads the relevant session, evaluates policies,
selects the model, builds the execution request, mediates tool calls,
and records the trace.

Smista-router is meant to run anywhere, both locally and on a remote
machine.

The router is responsible for:

- User authentication

- Session loading and persistence

- Task classification

- Routing policy evaluation

- Model selection

- Provider selection

- Fallback handling

- Context selection

- Tool call mediation

- Provider request execution

- Cost estimation when possible

- Trace creation

The router is the source of truth for routing decisions.

### Smista-core

Smista-core is the shared internal runtime used by the CLI and router.

It contains reusable domain types, validation logic, policy modes,
configuration structures, trace types, provider abstractions, and shared
execution primitives.

The core crate must avoid depending on terminal-specific or
server-specific concerns.

It is responsible for:

- Shared domain types

- Routing policy data structures

- Provider and model descriptors

- Task type definitions

- Tool permissions models

- Trace structures

- Configuration schemas

- Error types

- Common validation logic

TypeScript bindings must be generated for types useful for smista-SDK.

### Smista-SDK

Smista-SDK is a TypeScript and JavaScript SDK for interacting with
smista-router. It exposes the API types for working with smista-router.
This allows users to interact with the smista-router by implementing
their own frontend for Smista.ai.

Types must be automatically generated from smista-core when possible.

## Tech stack

Smista.ai is implemented primarily in Rust.

Rust is the main implementation language for the CLI, router, and core.

### Runtime

The system runs without external runtimes for normal local usage.

The CLI and router should be distributed as native binaries compiled
statically with musl, where practical.

The expected deployment model is:

- Smista-cli runs as a native CLI binary

- Smista-router runs locally as a native service

- Smista-core is a shared library as an internal Rust crate.

- Smista-SDK is a TypeScript NPM package.

### Rust Crates

The project should be organised as a Rust workspace. All crates must
stay under the crates/ folder.

Expected crates include:

- Smista-cli: CLI binary with all its logic.

- Smista-router: Backend binary running the entire smista-router stack.

- Smista-core: Library with shared types between backend and frontend.

- Smista-providers: LLM model providers.

- Smista-storage: Types to interact with the storage (backend).

- Smista-trace: Trace types and logic.

- Smista-web: HTTP JSON API web server implementation for smista-router.

### CLI Stack

The CLI should use Rust libraries suitable for building interactive
terminal applications; in particular, we will rely on ratatui, as codex
does.

The CLI stack should support:

- Command parsing with clap

- Interactive prompts

- Streaming output

- Syntax-highlighted diffs

- Path competition

- Terminal-safe rendering

- Cross-platform behaviour

- Config parsing with toml.

### Router stack

The router stack is implemented directly within smista-router and is
responsible for classifying user prompts and requests. In case of
prompts, the user input must be classified by intent, files, and skills,
and, through the user\'s policies, assigned to a specific model.

### Web Server stack

Smista-router should expose a web server.

The stack is implemented in the smista-web crate.

The server exposes a local HTTP API. The API is meant to be local, even
when installed on a machine exposed on the internet, so the external API
must be implemented on a separate server.

The router stack should support:

- HTTP JSON API endpoints exposed through axum.

- Request authentication

- Session tokens

- Structured JSON payloads

- Streaming responses when supported

- Provider abstraction

- Tool mediation

- Trace recording

### Provider stack

Smista-provider must implement a layer to interact with remote and local
LLM providers.

Interaction with providers should happen through rig where possible.

Initial providers are:

- OpenAI

- Anthropic

- Ollama

- OpenAI-compatible endpoints

### Storage stack

Smista-storage must implement a layer to define the storage entities and
the client to interact with the physical layer on SurrealDB.

It is responsible for:

- Defining the database entities

- Providing accessors for the database entities

- Embedding SurrealDB on local deployments.

- Connecting to an existing SurrealDB instance on SaaS deployments.

## Session Storage

Smista-router must persist user-scoped session state through a storage
layer.

The storage layer is responsible for users, sessions, authentication
tokens, session messages, routing decisions, tool calls, approvals,
plans, diffs, and trace metadata.

Smista.ai uses SurrealDB as the primary storage backend because it
supports both local embedded deployments and future server-side
deployments.

### Storage responsibilities

The storage layer is responsible for:

- Persisting users

- Persisting sessions

- Persisting session messages

- Persisting routing decisions

- Persisting selected context references

- Persisting tool calls and results

- Persisting approvals and rejections

- Persisting generated plans

- Persisting proposed and applied diffs

- Persisting trace metadata

- Persisting authentication tokens

- Enforcing user ownership at query boundaries.

### Database interface

The database must be accessed through an internal storage interface.

Application code should depend on storage traits rather than on
SurrealDB directly.

The storage interace should expose operations such as:

- Create user

- Get user by ID

- Get user by API key hash

- Create authentication token

- Validate authentication token

- Revoke authentication token

- Create session

- Get session by ID and user ID

- List sessions for user

- Update session metadata

- Archive session

- Delete session

- Append session message

- Append routing decision

- Append tool call

- Append approval decision

- Append trace event

- Get latest trace for session

- Get full session state

Database-specific query logic should be isolated in the SurrealDB
storage implementation.

### Entities

#### User

A user represents an identity that can own sessions.

In local-first deployments, a user may be created locally without a SaaS
account. In future SaaS deployments, the same entity can represent a
remote account.

Fields:

- id: unique user identifier

- api_key_hash: hash of the user\'s API key

- created_at

- updated_at

- disabled_at, optional

The raw API key must never be stored; only a secure hash of the API key
may be persisted.

#### AuthToken

An auth token represents a short-lived authentication router session.

The CLI uses an auth token after signing in with the user ID and API
key.

Fields:

- id: unique token identifier

- user_id: reference to the owning user (FK)

- token_hash: hash of the issued token

- created_at

- expires_at

- revoked_at, optional

The raw token must never be stored.

Expired or revoked tokens must be rejected by smista-router.

Expired token should be cleaned up from time to time.

### Session

A session represents a resumable user interaction with smista.ai.

Each session belongs to exactly one user.

Fields:

- id: UUIDv7 session identifier

- user_id: reference to the owning user

- title, optional

- created_at

- updated_at

- archived_at, optional

- deleted_at, optional

Session IDs must be globally unique.

A user must only be able to access sessions they own.

### SessionMessage

A session message stores a message exchanged during a session.

Fields:

- id

- session_id

- user_id

- role

- content

- created_at

- provider

- model

The user_id should be stored redundantly to simplify ownership checks
and scoped queries.

### SessionRoutingDecision

A session routing decision records which provider/model pair was
selected and why.

Fields:

- id

- session_id

- user_id

- task_type

- provider

- model

- matched_rule, optional

- fallback_used, optional

- override_used, optional

- reason

- created_at

### SessionContextReference

A context reference records what context was selected or excluded for a
task.

Fields:

- id

- session_id

- user_id

- path, optional

- kind

- included

- reason

- created_at

This entity should store references and metadata, not necessarily full
file contents.

Restricted file contents must not be persisted unless explicitly allowed
by policy.

### SessionToolCall

A session tool call records a tool request and its execution result.

Fields:

- id

- session_id

- user_id

- tool_name

- arguments

- status

- result, optional

- error, optional

- created_at

- completed_at, optional

Tool call arguments and results must be sanitised before persistence
when they may contain secrets.

### SessionApproval

A session approval records a user decision for an operation that
required confirmation.

Fields:

- id

- session_id

- user_id

- target_type

- target_id

- decision

- reason, optional

- created_at

Approvals may refer to tool calls, file writes, shell commands, network
access, or remote-provider context disclosure.

### SessionPlan

A session plan records a generated or approved execution plan.

Fields:

- id

- session_id

- user_id

- path

- status

- created_at

- updated_at

- approved_at, optional

- content_hash, optional

- content_snapshot, optional

### SessionDiff

A session diff records a proposed or applied file modification.

Fields:

- id

- session_id

- user_id

- path

- diff

- status

- created_at

- applied_at, optional

Diffs must be stored only after secret filtering and according to the
active privacy policy.

### TraceEvent

A trace event records a structured event during task execution.

Fields:

- id

- session_id

- user_id

- event_type

- payload

- created_at

Trace events should be append-only.

They provide the detailed execution history used by /trace.

### Future encryption support

The storage model should separate session metadata from session content.

This separation is required to support future end-to-end encrypted
sessions.

Session metadata may remain queryable, while sensitive session content
may later be encrypted before persistence.

### SurrealDB implementation

The SurrealDB implementation should map each entity to a dedicated
table.

Relations between entities should be represented using explicit
references.

SurrealDB-specific code must remain inside the storage implementation.

The rest of smista.ai should interact with storage through internal
traits and domain types.

## Authentication

Smista.ai has two separate authentication concerns:

- authentication between smista-cli (or any frontend) and smista-router

- authentication between smista-router and external model providers.

These authentication flows must remain separate.

Router authentication identifies the smista.ai user and protects
user-scoped resources such as sessions, traces, plans, and stored
execution metadata.

Provider authentication gives smista.ai access to external or local
model providers such as OpenAI, Anthropic, Ollama, or OpenAI-compatible
endpoints.

Provider credentials must never be used as smista.ai credentials.

Provider credentials must be provided by the user.

### Router authentication

Smisa-router must authenticate every request that accesses user-scoped
resources.

User-scoped resources include:

- Sessions

- Session messages

- Traces

- Stored routing decisions

- Tool calls

- Approvals

- Plan metadata

- Diff metadata

- Usage information

Each authenticated request must be associated with exactly one user.

A user must be able to access only the resources they own.

### User identity

A user is represented by a persistent record in storage.

In local-first deployments, this user may be created locally by the
router without requiring a SaaS account.

In future SaaS deployments, the same user model may be associated with a
remote account, but would require a proxy in front.

A user record contains:

- id

- api_key_hash

- created_at

- updated_at

- disabled_at, optional

The raw api key must never be stored

Only a secure hash of the api key may be persisted.

### API key

Smista.ai uses API-key-based authentication between the frontend and the
router.

The API key is a long-lived credential used to obtain a short-lived auth
token.

The API key must:

- Be generated by smista-router

- Be shown to the client only once

- Be stored by the client securely where possible

- Be stored server-side only as a hash

- Never appear in logs, traces, errors, or debug output

The API key format is:

sk-smista-api01-\<random-96-alphanumeric-bytes\>

Where \`01\` is the API key version.

The random part must be generated using a cryptographically secure
random generator.

The hash of the API key must be unique.

### User bootstrap

Smista-router must expose an API endpoint to register users and generate
API keys. The user ID and the API key must then be returned to the
calling user.

### Sign-in flow

The CLI authenticates with the router using the user ID and the API key.

The sign-in flow is:

1.  Smista-cli sends the user ID and API key to smista-router

2.  Smista-router hashes the provided API key.

3.  Smista-router compares the hash aginst the stored API key hash.

4.  If valid, smista-router creates a short-lived auth token.

5.  Smista-router returns the auth token to the CLI.

6.  Smista-cli uses the auth token for subsequent router requests.

7.  The auth token is discarded when the CLI session ends or when it
    expires.

### Auth token

An auth token represents a short-lived authenticated router session.

Auth tokens are used after sin-in and must be included in authenticated
requests to smista-router.

An auth token contains:

- id

- user_id

- token_hash

- created_at

- expires_at

- revoked_at, optional

The raw token must never be stored.

Only a secure hash of the token may be persisted.

Expired, revoked, missing, or invalid tokens must be rejected.

The format of the token for the user is:

\<64 alphanumeric lowercase random bytes\>

### Request authentication

For each authenticated request, smista-router must:

1.  Read the provided auth token.

2.  Hash the token.

3.  Look up the matching non-expired, non-revoked token.

4.  Resolve the associated user.

5.  Execute the request only in the scope of that user.

All user-scoped queries must be filtered by user_id.

For example, loading a session must require both:

- session_id

- Authenticated user_id

The router must reject the request if the session does not belong to the
authenticated user.

### Client-side credential storage

Smista-cli should store router credentials securely.

Preferred storage order:

1.  OS keychain, where available

2.  Credential helper

3.  Local configuration file with restricted permissions

The CLI may store:

- User ID

- Router URL

- API key, if needed for future sign-ins

The CLI should avoid persisting short-lived auth tokens.

Auth tokens should normally live only for the duration of the active CLI
session.

### Provider authentication

Provider authentication is separate from router authentication.

Provider credentials are used to call external or local model providers.

Supported provider credential sources include:

- Environment variables

- Local configuration (.smista/config.toml)

- OS keychains

- Credential helpers

- .smista/secrets

- Unauthenticated local endpoints when appropriate.

Provider credentials must never be stored in session records, traces,
routing decisions, or tool call logs.

Provider credentials must not be exposed to models.

### Secrets handling

Smista.ai must avoid exposing secrets in:

- Logs

- Traces

- Error messages

- Tool call arguments

- Tool call results

- Provider payload previews

- Debug output

Secrets should be redacted before persistence.

The router must treat API keys, provider keys, auth tokens, and secret
references as sensitive values.

Consider using the \"secrecy\" crate to hide secrets.

### Revocation and expiration

Smista-router must support revoking auth tokens.

Smista-router should support revoking or disabling API keys.

When a token is revoked or expires, all further requests using that
token must be rejected.

When a user is disabled, all API keys and active tokens associated with
that user must be revoked.

## Model interfaces

Smista.ai must expose a common internal interface for all supported
models.

The purpose of the model interface is to let smista-router execute
requests against different providers without coupling routing logic to
provider-specific APIs.

Routing, policy evaluation, context selection, and tool permission
checks must occur before the model provider is invoked (and are out of
scope).

Provider-specific request formatting, streaming handling, tool-call
translation, and usage parsing must be isolated behind model/provider
adapters.

### Model abstraction

Each model must be represented by a common internal interface.

The interface should support:

- Sending a prompt or message list

- Streaming responses when supported

- Returning tool call requests when supported

- Returning final assistant responses

- Returning usage information when available

- Returning structured provider errors

- Exposing model capabilities

- Exposing authentication requirements.

The router should interact with models only through this abstraction.

Routing logic must not depend on Provider-specific APIs.

### Model trait

The Rust implementation should define a trait representing a callable
model.

### Model descriptor

Each configured model must expose a descriptor.

The descriptor defines the static and runtime capabilities of the model.

A model descriptor should include:

- provider

- model

- display_name

- local

- requires_api_key

- supports_streaming

- supports_tools

- supports_json_output

- supports_system_prompt

- supports_images

- supports_reasoning

- max_context_tokens

- max_output_tokens, optional

- input_cost_per_million_tokens, optional

- output_cost_per_million_tokens, optional

- default_parameters

- provider_options, optional

These capabilities are used by the router to select models, validate
routing rules, estimate costs, and prepare requests.

For example, a task requiring tool calls must not be routed to a model
that does not support tools unless the policy explicitly allows degraded
execution.

### Authentication requirements

Each model must declare whether it requires provider credentials.

A model may:

- Be unauthenticated

- Have optional authentication

- API-key authenticated

- OpenAI-compatible endpoint

The model descriptor must expose this through \`requires_api_key\` or a
richer authentication mode.

For example:

pub enum ModelAuthRequirement {\
None,\
ApiKey,\
OptionalApiKey,\
Custom(String),\
}

### Use of rig.rs

Smista.ai should use Rig as the primary integration layer for model
providers where practical.

rig provides a Rust-native abstraction for LLM applications and supports
multiple model providers behind common interfaces. Its documentation
lists native provider integrations including Anthropic, Azure OpenAI,
Ollama, OpenAI, OpenRouter, Gemini, Mistral, Groq, xAI, and others; it
also allows custom integrations through CompletionModel and
EmbeddingModel traits.

Smista.ai should use rig for:

- Provider client construction

- Completion calls

- Streaming calls where supported

- Tool-call support where supported

- Provider-specific request/response handling

- Usage extraction where available

However, rig should remain an implementation detail of the provider
adapter layer.

The rest of smista.ai should depend on smista.ai domain traits and
types, not directly on rig provider types.

This keeps the internal architecture stable even if a provider requires
custom code or if rig does not expose a needed capability.

### Tool calls

Models that support tool calls may return structured tool-call requests.

The model interface must not execute tools directly.

Instead, tool calls must be returned to smista-router.

The router is responsible for:

- Validating tool calls against active permissions

- Asking the CLI for approval when required

- Executing allowed tool calls through the tool runtime

- Returning tool results to the model when needed

- Recording the full tool flow in the trace

This ensures that all model-requested actions remain mediated by
smista.ai.

### Streaming

If a model supports streaming, the model descriptor must expose that
capability.

The model interface should provide a streaming API that returns
structured stream events.

Stream events may include:

- Text delta

- Tool call delta

- Reasoning delta, if supported

- Usage update, if available

- Error

- End of response

The CLI should render streaming output when supported by the selected
provider.

If streaming is not supported, the router should fall back to a normal
completion request.

### Usage and cost metadata

The model interface should return usage metadata when the provider
exposes it.

Usage metadata may include:

- Input tokens

- Output tokens

- Cached tokens

- Reasoning tokens

- Total tokens

- Estimated cost

- Actual cost when available

The router should record this metadata in the trace.

When pricing is configured for a model, smista.ai may estimate costs
before or after execution.

### Error handling

Provider-specific errors must be normalised into internal error types.

The model interface should distinguish between:

- Authentication errors

- Missing credentials

- Invalid credentials

- Rate limits

- Context length errors

- Unsupported capability errors

- Provider unavailable errors

- Timeout errors

- Invalid request errors

- Unknown provider errors

These errors are used by the router to decide whether fallback is
possible.

### Fallback behaviour

Fallback is owned by smista-router, not by the model interface.

If a model call fails with a fallback-eligible error, the router may
select the next configured fallback model, from the user\'s policy.

Fallback decisions must be recorded in the trace.

The model adapter should only report the failure accurately.

### Implementation principle

The model interface must preserve a strict separation between:

- Routing policy

- Provider integration

- Credential handling

- Tool execution

- User interaction

<!-- -->

- The router selects the model.

- The provider adapter invokes the model.

- The tool runtime executes tools.

- The CLI interacts with the user.

This separation allows smista.ai to support multiple model providers
while keeping routing deterministic, inspectable, and independent from
provider-specific APIs.

## API

smista-router must expose a JSON REST API.

The API is used by smista-cli, future SDKs, and possible future frontend
clients to authenticate, manage sessions, preview routes, execute model
requests, inspect traces, and manage usage information.

The API must use meaningful HTTP methods:

- GET for reading resources

- POST for creating resources or executing commands

- PUT for replacing or updating resources

- DELETE for deleting or revoking resources

All request and response bodies must use JSON unless explicitly
documented otherwise.

### API design principles

The API must follow these principles:

- Paths must be meaningful and resource-oriented.

- Authentication endpoints must live under /auth.

- Session endpoints must live under /sessions.

- Routing endpoints must live under /routes.

- Trace endpoints must live under /traces.

- Provider/model endpoints must live under /providers or /models.

- HTTP status codes must reflect the actual outcome.

- Error responses must be structured and actionable.

- Secrets must never be returned in API responses.

- API keys, auth tokens, and provider credentials must never be logged.

### Headers

Authenticated requests must use headers for authentication and
credentials.

Router authentication should use:

Authorization: Bearer \<session-token\>

The long-lived smista API key should be sent only to authentication
endpoints, for example:

X-Smista-Api-Key: \<apikey\>

Provider credentials should be sent as headers when required by the
selected model or provider:

X-Smista-Provider-\<Provider_Name\>-Api-Key: \<apikey\>

For instance:

X-Smista-Provider-Anthropic-Api-Key: \<apikey\>

Provider credentials should only be sent when required.

The router must treat all credential headers as sensitive values.

Credential headers must not be logged, stored in traces, exposed in
errors, or forwarded to models as context.

### Authentication API

Authentication endpoints must live under \`/auth\`.

#### Create user

Creates a user and returns the user ID and the API key.

POST /auth/bootstrap

Response:

{\
\"user_id\": \"user:abc123\",\
\"api_key\": \"sk-smista-api01-\...\"\
}

#### Sign in

POST /auth/sign-in

Headers:

X-Smista-Api-Key: \<apikey\>

Body:

{\
\"user_id\": \"user:abc123\"\
}

Response:

{\
\"token\": \"st\_\...\",\
\"expires_at\": \"2026-05-25T12:00:00Z\"\
}

#### Sign out

Revokes the current session token.

POST /auth/sign-out

Headers:

Authorization: Bearer \<session-token\>

Response:

{\
\"revoked\": true\
}

#### Get current user

Returns the authenticated user

GET /auth/me

Headers:

X-Smista-Api-Key: \<apikey\>

Response:

{\
\"sessions\": \[\
{\
\"id\": \"018f2f4c-\...\",\
\"title\": \"Refactor auth middleware\",\
\"created_at\": \"2026-05-25T09:00:00Z\",\
\"updated_at\": \"2026-05-25T09:30:00Z\",\
\"archived\": false\
}\
\]\
}

#### Create session

Create a new session.

POST /sessions

Headers:

Authorization: Bearer \<session-token\>

Body:

{\
\"title\": \"Refactor auth middleware\"\
}

Title is mandatory. The frontend should provide random names if the user
doesn\'t want to provide one.

Response:

{\
\"session\": {\
\"id\": \"018f2f4c-\...\",\
\"title\": \"Refactor auth middleware\",\
\"created_at\": \"2026-05-25T09:00:00Z\",\
\"updated_at\": \"2026-05-25T09:00:00Z\"\
}\
}

#### Get session

Get a session to be resumed.

GET /sessions/{session_id}

Headers:

Authorization: Bearer \<session-token\>

Response:

{\
\"session\": {\
\"id\": \"018f2f4c-\...\",\
\"title\": \"Refactor auth middleware\",\
\"created_at\": \"2026-05-25T09:00:00Z\",\
\"updated_at\": \"2026-05-25T09:30:00Z\",\
\"messages\": \[\],\
\"metadata\": {}\
}\
}

#### Update session

Update session title or archive

PUT /sessions/{session_id}

Headers:

Authorization: Bearer \<session-token\>

Body:

{\
\"title\": \"New session title\",\
\"archived\": false\
}

Response: empty.

#### Delete session

Delete a session

DELETE /sessions/{session_id}

Headers:

Authorization: Bearer \<session-token\>

Response:

{\
\"deleted\": true\
}

### Execution API

Execution endpoints run tasks inside a session.

#### Execute a task

POST /sessions/{session_id}/execute

Headers:

Authorization: Bearer \<session-token\>

X-Smista-Provider-{provider}-Api-Key: \<apikey\>

Provider API key can be optional, can be more than one, and depends on
the user\'s configuration.

Body:

{

\"input\": {

\"text\": \"refactor the auth middleware\",

\"command\": \"edit\",

\"explicit_model\": null

},

\"workspace\": {

\"root\": \"/Users/christian/project\",

\"git_branch\": \"main\",

\"git_diff\": \"\...\",

\"referenced_paths\": \[

\"src/auth/middleware.rs\"

\],

\"active_file\": null

},

\"policy\": {

\"version\": 1,

\"source\": \"merged\",

\"routing\": {

\"default_model\": \"anthropic/claude-sonnet\",

\"rules\": \[

{

\"match\": {

\"task_type\": \"edit\",

\"path\": \"src/auth/\*\*\"

},

\"use\": \"anthropic/claude-sonnet\"

}

\],

\"fallbacks\": {

\"anthropic/claude-sonnet\": \[

\"openai/gpt-5.5-thinking\",

\"ollama/qwen2.5-coder\"

\]

}

},

\"permissions\": {

\"file_read\": \"allow\",

\"file_write\": \"ask\",

\"shell\": \"ask\",

\"network\": \"deny\"

},

\"privacy\": {

\"restricted_paths\": \[

\".env\",

\"secrets/\*\*\",

\"target/\*\*\"

\],

\"remote_model_requires_approval_for_restricted_context\": true

}

},

\"local_preferences\": {

\"auto_apply\": false,

\"stream\": true,

\"local_only\": false,

\"no_network\": false

},

\"providers\": \[

{

\"id\": \"anthropic\",

\"models\": \[

{

\"model\": \"claude-sonnet\",

\"requires_api_key\": true,

\"credential_available\": true

}

\]

},

{

\"id\": \"ollama\",

\"models\": \[

{

\"model\": \"qwen2.5-coder\",

\"requires_api_key\": false,

\"credential_available\": true

}

\]

}

\],

\"context\": {

\"messages\": \[

{

\"role\": \"user\",

\"content\": \"Previous relevant message\...\"

}

\],

\"files\": \[

{

\"path\": \"src/auth/middleware.rs\",

\"content\": \"\...\",

\"content_hash\": \"sha256:\...\"

}

\],

\"instructions\": \[

{

\"source\": \"SMISTA.md\",

\"content\": \"\...\"

}

\],

\"skills\": \[\],

\"prompt_template\": null

}

}

Response:

{

\"status\": \"completed\",

\"message\": {

\"role\": \"assistant\",

\"content\": \"\...\"

},

\"routing\": {

\"task_type\": \"edit\",

\"provider\": \"anthropic\",

\"model\": \"claude-sonnet\",

\"matched_rule\": \"edit + src/auth/\*\* -\> anthropic/claude-sonnet\",

\"fallback_used\": false,

\"override_used\": false

},

\"context\": {

\"included\": \[

\"src/auth/middleware.rs\",

\"SMISTA.md\",

\"current git diff\"

\],

\"excluded\": \[

\".env\",

\"secrets/\*\*\"

\]

},

\"usage\": {

\"input_tokens\": 1200,

\"output_tokens\": 500,

\"estimated_cost\": 0.08,

\"currency\": \"USD\"

},

\"trace_id\": \"trace:xyz\"

}

If the model requests a tool call requiring approval, the response may
return a pending approval instead of a final assistant message.

#### Stream task

Streaming may be exposed through a dedicated endpoint:

POST /sesssions/{session_id}/stream

Headers:

Authorization: Bearer \<session-token\>

X-Smista-Provider-{provider}-Api-Key: \<apikey\>

The request body is the same as /execute.

The response should use a documented streaming format, such as
Server-Sent Events.

Each stream event should be structured like this:

{\
\"type\": \"text_delta\",\
\"delta\": \"The first step is\...\"\
}

Supported stream event types may include:

- text_delta

- tool_call_requested

- approval_required

- tool_result

- usage

- error

- done

### Route preview API

#### Preview route

Get the preview of routing for a prompt.

POST /sessions/{session_id}/preview

Headers:

Authorization: Bearer \<session-token\>

Body: same as execute.

Response:

{

\"task_type\": \"review\",

\"provider\": \"openai\",

\"model\": \"gpt-5.5-thinking\",

\"matched_rule\": \"task.review -\> openai/gpt-5.5-thinking\",

\"included_context\": \[

\"current git diff\",

\"SMISTA.md\"

\],

\"excluded_context\": \[

\".env\",

\"target/\*\*\"

\],

\"estimated_cost\": {

\"min\": 0.03,

\"max\": 0.09,

\"currency\": \"USD\"

},

\"required_permissions\": \[

{

\"permission\": \"read_repository\",

\"mode\": \"allow\"

},

{

\"permission\": \"write_files\",

\"mode\": \"ask\"

}

\]

}

The endpoint must not call the selected model.

### Approvals API

Approvals are used when model-requested actions require user
confirmation.

#### Submit approval decision

POST /sessions/{session_id}/approvals/{approval_id}

Headers:

Authorization: Bearer \<session-token\>

Body:

{\
\"decision\": \"approved\",\
\"reason\": null\
}

Valid decisions are:

- approved

- rejected

Response:

{\
\"approval_id\": \"approval:abc\",\
\"decision\": \"approved\"\
}

### Trace API

Trace endpoints expose execution traces.

#### Get latest trace for session

GET /sessions/{session_id}/traces/latest

Headers:

Authorization: Bearer \<session-token\>

Response

{

\"trace\": {

\"id\": \"trace:xyz\",

\"session_id\": \"018f2f4c-\...\",

\"task_type\": \"edit\",

\"provider\": \"anthropic\",

\"model\": \"claude-sonnet\",

\"matched_rule\": \"edits under src/auth/\*\* use claude-sonnet\",

\"events\": \[\]

}

}

#### Get trace by ID

GET /sessions/{session_id}/traces/{trace_id}

Headers:

Authorization: Bearer \<session-token\>

Response: same as traces/latest.

### Providers and models API

#### List providers

GET /llm/providers

Response:

{

\"providers\": \[

{

\"id\": \"openai\",

\"display_name\": \"OpenAI\",

\"configured\": true

},

{

\"id\": \"ollama\",

\"display_name\": \"Ollama\",

\"configured\": true

}

\]

}

#### List models

GET /llm/models

Response:

{

\"models\": \[

{

\"provider\": \"openai\",

\"model\": \"gpt-5.5-thinking\",

\"local\": false,

\"requires_api_key\": true,

\"supports_streaming\": true,

\"supports_tools\": true,

\"supports_json_output\": true,

\"max_context_tokens\": 200000

},

{

\"provider\": \"ollama\",

\"model\": \"qwen2.5-coder\",

\"local\": true,

\"requires_api_key\": false,

\"supports_streaming\": true,

\"supports_tools\": false,

\"max_context_tokens\": 32768

}

\]

}

### Usage API

#### Get session usage

Get /sessions/{session_id}/usage

Response:

{

\"session_id\": \"018f2f4c-\...\",

\"usage\": {

\"total\": {

\"input_tokens\": 12000,

\"output_tokens\": 4200,

\"total_tokens\": 16200,

\"estimated_cost\": 0.42,

\"currency\": \"USD\"

},

\"by_model\": \[

{

\"provider\": \"openai\",

\"model\": \"gpt-5.5-thinking\",

\"input_tokens\": 8000,

\"output_tokens\": 2200,

\"total_tokens\": 10200,

\"estimated_cost\": 0.31,

\"currency\": \"USD\",

\"request_count\": 3

}

\],

\"by_task_type\": \[

{

\"task_type\": \"plan\",

\"input_tokens\": 4000,

\"output_tokens\": 1200,

\"estimated_cost\": 0.18,

\"request_count\": 1

}

\]

}

}

### Error responses

Errors must use a consistent JSON format.

Example:

{

\"error\": {

\"code\": \"missing_provider_credentials\",

\"message\": \"The selected model requires provider credentials, but
none were provided.\",

\"details\": {

\"provider\": \"anthropic\",

\"model\": \"claude-sonnet\"

}

}

}

Error responses must not expose secrets.

### HTTP status code

The API must use meaningful HTTP status codes.

Required status codes include:

- 200 OK for successful reads or completed commands

- 201 Created for created resources

- 202 Accepted for long-running or pending operations

- 204 No Content for successful deletion without body

- 400 Bad Request for invalid request payloads

- 401 Unauthorised for missing or invalid authentication

- 403 Forbidden for authenticated requests blocked by ownership or
  policy

- 404 Not Found for missing resources

- 409 Conflict for conflicting resource state

- 422 Unprocessable Entity for valid JSON that fails domain validation

- 429 Too Many Requests for rate limits

- 500 Internal Server Error for unexpected server errors

- 502 Bad Gateway for provider errors

- 503 Service Unavailable for unavailable providers or storage

- 504 Gateway Timeout for provider timeouts

### Security requirements

The API must follow these security requirements:

- Router session tokens must be provided through headers.

- Smista API keys must be provided through headers.

- Provider credentials must be provided through headers.

- Credentials must not be accepted in URL query parameters.

- Credentials must not be written to logs.

- Credentials must not be written to traces.

- Credentials must not be included in the model context.

- All user-scoped resources must be checked against the authenticated
  user.

- Provider credentials must be used only for the selected provider
  request.

- Provider credentials must be discarded after the request unless
  explicit credential storage is configured.

### Versioning

The API should be versioned.

All the endpoints must stay under:

/api/v1

For example:

/api/v1/auth/sign-in

## CLI Configuration and policies

Smista-cli uses configuration files to define providers, models, routing
rules, tool permissions, privacy constraints, prompt templates, skills,
and local preferences.

Configuration must be deterministic, versionable, and inspectable.

The default configuration format is TOML.

Global configuration is stored in the user configuration directory:

\~/.config/smista/config.toml \# POSIX

C:\\Users\\\$USER\\.smista\\config.toml \# Windows

Workspace configuration is stored instead at:

.smista/config.toml

Workspace configuration overrides global configuration according to
documented precedence rules.

### Configuration principles

Configuration and policies must follow these principles:

- Routing must be deterministic.

- Routing must not depend on LLM judgment.

- Policies must be readable and versionable.

- Project policies must be safe to commit when they do not contain
  secrets.

- Secrets must be referenced, not stored inline.

- Local preferences must not unexpectedly override project safety
  policies.

- Ambiguous routing rules must be rejected or require explicit priority.

- Every routing decision must be explainable in the trace.

### Configuration layers

Smista.ai should support multiple configuration layers:

- system defaults

- global user configuration

- project configuration

- local uncommitted preferences

- runtime command override

The precedence order is (most to least):

1.  runtime command override

2.  local uncommitted preferences

3.  project configuration

4.  global user configuration

5.  system defaults

However, safety policies may define non-overridable restrictions.

For example, a project may forbid sending secrets/\*\* to remote models.
A local user preference must not silently bypass this restriction.

### Policy model

A routing policy is a deterministc list of routing rules.

Each rule may match on:

- Intent

- Skill

- File path

- Provider

- Model capability

- Local/remote execution

- Tool requirements

- Privacy constraints

A rule produces a routing decision, such as:

- Selected provider/model pair

- Fallback chain

- Required permissions

- Local-only requirement

- Remote-provider restriction

- Cost limit

### Task intents

Smista.ai supports a fixed set of built-in task intents.

Initial intents are:

chat\
plan\
edit\
review\
summarize\
prompt\
skill

The detected intent may come from:

- Explicit CLI command

- Explicit slash command

- Prompt template metadata

- Skill metadata

- Deterministic text patterns

- Path/context signals

Explicit commands always win over automatic classification.

For example:

/plan refactor the auth middleware

Must be classified as:

plan

even if the text also contains words associated with editing.

### Intent detection

Intent detection must be deterministic.

The classifier should use ordered rules rather than an LLM.

A classification rule may match:

- Explicit command

- Prompt prefix

- Keywords

- Required files

- Current workflow state

- Skill invocation

- Prompt template invocation

\[\[classification.rules\]\]\
intent = \"review\"\
priority = 10\
keywords = \[\"review\", \"audit\", \"check\", \"inspect\"\]\
requires_any_context = \[\"git_diff\", \"pull_request\"\]\
\
\[\[classification.rules\]\]\
intent = \"edit\"\
priority = 20\
keywords = \[\"change\", \"modify\", \"refactor\", \"fix\",
\"implement\"\]

If no classification rule matches, the default intent is:

\[classification\]

default_intent = \"chat\"

The classifier must return:

- Detected intent

- Matched classification rule

- Confidence category, if useful

- Reason

- Whether the intent was explicit or inferred

The confidence category must not be probabilistic unless it is derived
from deterministic scoring.

[Priority: the lower the value, the higher.]{.underline}

### Router client configuration

smista-cli must support configuration for connecting to smista-router.

The CLI and SDK clients use this configuration. It is different from
router runtime configuration.

Router client configuration defines where the router API is located, how
the CLI authenticates to it, and whether the CLI should automatically
start a local router.

Example:

\[router\]\
url = \"http://127.0.0.1:7331\"\
auto_start = true\
connect_timeout_ms = 5000\
request_timeout_ms = 120000\
auth_source = \"keychain\"

### Routing rules

Routing rules define which model should handle a task.

A routing rule may match by intent:

\[\[routing.rules\]\]\
name = \"plan with strongest reasoning model\"\
priority = 10\
intent = \"plan\"\
model = \"openai/gpt-5.5-thinking\"\
fallbacks = \[\"anthropic/claude-sonnet\"\]

A routing rule may match by skill:

\[\[routing.rules\]\]\
name = \"use local model for changelog skill\"\
priority = 20\
skill = \"changelog\"\
model = \"ollama/qwen2.5-coder\"\
fallbacks = \[\"openai/gpt-5.5-mini\"\]

A routing rule may match by path:

\[\[routing.rules\]\]\
name = \"auth code uses Claude\"\
priority = 30\
paths = \[\"src/auth/\*\*\"\]\
intent = \"edit\"\
model = \"anthropic/claude-sonnet\"\
fallbacks = \[\"openai/gpt-5.5-thinking\"\]

A routing rule may combine match conditions:

\[\[routing.rules\]\]\
name = \"review security-sensitive code locally\"\
priority = 5\
intent = \"review\"\
paths = \[\"src/crypto/\*\*\", \"src/auth/\*\*\"\]\
local_only = true\
model = \"ollama/qwen2.5-coder\"

### Match semantics

All fields inside one rule are combined with **AND**.

For example:

intent = \"edit\"\
paths = \[\"src/auth/\*\*\"\]

Means:

match when intent is edit AND at least one selected path matches
src/auth/\*\*.

Lists inside one field use **OR**.

For example:

paths = \[\"src/auth/\*\*\", \"src/security/\*\*\"\]

Means:

src/auth/\*\* OR src/security/\*\*

A rule matches only when all defined match fields match.

Undefined fields are ignored.

### Rule precedence

When multiple routing rules match, smista-router must select exactly one
rule.

The precedence order is:

1.  Explicit model override

2.  Rules with lower priority value (1,2,3,...)

3.  More specific rules

4.  Configuration order

Specificity is calculated deterministically.

Specificity order:

skill + path + intent\
\> skill + path\
\> path + intent\
\> skill\
\> path\
\> intent\
\> default

If two rules have the same priority and the same specificity,
configuration validation should fail unless ordering is explicitly
allowed.

### Default route

A routing policy must define a default route.

\[routing.default\]\
model = \"openai/gpt-5.5-mini\"\
fallbacks = \[\"ollama/qwen2.5-coder\"\]

The default route is used when no routing rule matches.

### Model references

Models should be referenced using

provider/model

syntax.

Examples:

openai/gpt-5.5-thinking\
anthropic/claude-sonnet\
ollama/qwen2.5-coder

This keeps routing rules compact and avoids ambiguous model names.

### Providers and models

Providers and models are configured separately from the routing rules.

Example:

\[providers.openai\]\
type = \"openai\"\
base_url = \"https://api.openai.com/v1\"\
api_key_env = \"OPENAI_API_KEY\"\
\
\[providers.anthropic\]\
type = \"anthropic\"\
api_key_env = \"ANTHROPIC_API_KEY\"\
\
\[providers.ollama\]\
type = \"ollama\"\
base_url = \"http://localhost:11434\"

Models expose capabilities:

\[models.\"openai/gpt-5.5-thinking\"\]\
provider = \"openai\"\
name = \"gpt-5.5-thinking\"\
requires_api_key = true\
local = false\
supports_streaming = true\
supports_tools = true\
supports_json_output = true\
supports_reasoning = true\
max_context_tokens = 200000\
\
\[models.\"ollama/qwen2.5-coder\"\]\
provider = \"ollama\"\
name = \"qwen2.5-coder\"\
requires_api_key = false\
local = true\
supports_streaming = true\
supports_tools = false\
max_context_tokens = 32768

Routing must validate model capabilities before execution.

For example, if a workflow requires tool calls, the selected model must
support tool calls unless the policy allows degraded execution.

### Privacy policies

Privacy policies define what context may be sent to which model classes.

Example:

\[privacy\]\
restricted_paths = \[\
\".env\",\
\"secrets/\*\*\",\
\"\*.pem\",\
\"\*.key\"\
\]\
\
\[privacy.remote\]\
mode = \"ask\"\
blocked_paths = \[\
\".env\",\
\"secrets/\*\*\"\
\]\
\
\[privacy.local\]\
mode = \"allow\"

Remote models must not receive restricted files unless the active policy
explicitly allows it and user approval has been granted.

### Tool permissions

Tool permissions define what models may request and what requires
approval.

Example:

\[tools.permissions\]\
file_read = \"allow\"\
file_write = \"ask\"\
shell = \"ask\"\
network = \"deny\"\
git = \"allow\"

Skill-specific or rule-specific permissions may narrow the default
permissions.

They must not silently expand project-level restrictions unless
explicitly allowed.

The CLI may ask the user to expand permissions by asking not to be
prompted again about a tool.

### Validation

Configuration validation must happen before execution.

Validation must check:

- Unknown providers

- Unknown models

- Unknown intents

- Unknown skills when statically known

- Invalid glob patterns

- Duplicate rule names

- Missing default route

- Invalid fallback references

- Unsupported capability requirements

- Ambiguous routing rules

- Unsafe override behavior

- Invalid permission values

- Secret values stored inline where forbidden

Invalid configuration must produce actionable errors.

## Router configuration

smista-router must be configured through a dedicated router
configuration section.

Router configuration defines how the router process runs, how it exposes
its HTTP API, and how it connects to storage, how authentication works,
and which runtime limits are enforced.

Router configuration is separate from routing policy.

The routing policy decides which model should execute a task; router
configuration decides how smista-router itself operates.

The default configuration format is TOML.

Global configuration is stored in the user configuration directory:

\~/.config/smista/router.toml \# POSIX

C:\\Users\\\$USER\\.smista\\router.toml \# Windows

Workspace configuration is stored instead at:

.smista/router.toml

Example:

\[router\]\
host = \"127.0.0.1\"\
port = 7331\
\
\[router.storage\]\
engine = \"surrealdb\"\
mode = \"embedded\"\
path = \".smista/db\"\
namespace = \"smista\"\
database = \"local\"\
\
\[router.auth\]\
token_ttl_seconds = 86400\
api_key_version = \"01\"\
local_bootstrap_enabled = true\
\
\[router.limits\]\
max_request_body_bytes = 10485760\
max_context_bytes = 5242880\
max_concurrent_requests = 8\
request_timeout_ms = 120000\
provider_timeout_ms = 180000\
tool_timeout_ms = 60000\
\
\[router.logging\]\
level = \"info\"\
format = \"compact\"\
redact_secrets = true

\[router.logging\]\
level = \"info\"\
format = \"compact\"

\[router.cors\]\
enabled = true\
allowed_origins = \[\"https://app.smista.ai\"\]

\[router.retention\]\
trace_retention_days = 90\
session_retention_days = 365\
deleted_session_retention_days = 30

### Router server

The router server configuration controls how the HTTP API is exposed.

\[router\]\
host = \"127.0.0.1\"\
port = 7331

### Storage

Router storage configuration defines where users, sessions, tokens,
traces, and execution metadata are persisted.

\[router.storage\]\
engine = \"surrealdb\"\
mode = \"embedded\"\
path = \".smista/db\"\
namespace = \"smista\"\
database = \"local\"

### Authentication

Router auth configuration controls API key bootstrap, session token TTL,
and authentication behavior.

\[router.auth\]\
token_ttl_seconds = 86400\
api_key_version = \"01\"\
local_bootstrap_enabled = true

### Runtime limits

Runtime limits protect the router from oversized requests, runaway
tools, excessive context, and hanging provider calls.

\[router.limits\]\
max_request_body_bytes = 10485760\
max_context_bytes = 5242880\
max_concurrent_requests = 8\
request_timeout_ms = 120000\
provider_timeout_ms = 180000\
tool_timeout_ms = 60000

### Logging

Router logging configuration controls log level and format.

\[router.logging\]\
level = \"info\"\
format = \"compact\"

### CORS

CORS should be disabled by default.

It is only required for browser-based clients or future web dashboards.

\[router.cors\]\
enabled = true\
allowed_origins = \[\"https://app.smista.ai\"\]

CORS must never be enabled with unrestricted origins in production.

### Trace retention

Router configuration may define retention policies for sessions and
traces.

\[router.retention\]\
trace_retention_days = 90\
session_retention_days = 365\
deleted_session_retention_days = 30

### Validation

Router configuration must be validated at startup.

Validation must check:

- Invalid host or port

- Invalid router mode

- Missing storage configuration

- Unsupported storage engine

- Invalid storage mode

- Missing remote storage URL when required

- Unsafe public binding in local mode

- Enabled local bootstrap in remote/SaaS mode

- Invalid timeout values

- Invalid request size limits

- Unsafe CORS configuration

- Secret values stored inline where forbidden

An invalid router configuration must prevent the router from starting.

The error must explain which field is invalid and how to fix it.

## CLI Commands

smista-cli exposes a command-line executable named smista.

The CLI must support two execution modes:

- One-shot execution

- Interactive session execution

In interactive sessions, all commands must be executed using
slash-command syntax:

/\<command\>

For example:

/plan refactor the auth middleware

/review

/trace

/resume 018f2f4c-...

The only exception is the top-level shell invocation:

smista \<prompt\>

This starts a session, sends the provided prompt, and executes it using
the active routing policy.

### Start interactive session

Run just by invoking the binary.

Starts an interactive smista.ai session.

The CLI must:

- Connect to smista-router

- Authenticate with the router

- Load the active workspace configuration

- Create or resume a session

- Wait for user input

### Run one-shot prompt

smista refactor the auth middleware

Starts a session and executes the prompt immediately.

The CLI must:

- Load the active workspace configuration

- Connect to the router

- Authenticate

- Create or resume a session

- Send the prompt to the router

- Display the response

- Persist the session and trace

This form is equivalent to starting an interactive session and
submitting the prompt as a normal user message.

### Initialize workspace

smista init

Initializes smista.ai configuration in the current project.

This command may create:

.smista/\
.smista/config.toml

### Show version

smista \--version

### Show help

smista \--help

### Interactive slash commands

#### Init

/init

Initialises the smista.ai markdown file in the current project by
creating **SMISTA.md**, containing useful information for the agents.

#### Help

/help

Displays all available commands

#### /new or /clear

Creates a new session.

The current session remains persisted and can be resumed later.

#### /resume

/resume

If passed without arguments shows all the resumable sessions.

/resume \<session-id\>

Resume given session. The current session is saved before switching.

#### /archive

/archive \<session-id\>

Archive the given session.

#### /plan

/plan \<prompt\>

Runs the prompt as a planning task.

#### /edit

/edit \<prompt\>

Runs the prompt as an edit taks.

#### /review

/review \<prompt\>

Runs the prompt as a review taks.

#### /summarize

/summarize \<prompt\>

/summarise \<prompt\> \# same as

Runs the prompt as a summarize task.

#### /prompt

/prompt \<template-name\> \[args...\]

Runs a configured prompt template.

#### /skills

/skills

Show configured skills

#### /route

/route \<prompt\>

Get routing preview for a certain prompt based on the user\'s policies.

#### /trace

/trace \<latest\|trace-id\>

Get the trace of the given session.

#### /why

/why

Explains why the last model was selected.

#### /model

/model \<provider/model\|default\>

Sets an explicit model override for the current session.

Disable the specified model by passing \"default\".

#### /models

/models

Gets the available models with their capabilities.

#### /providers

/providers

Lists the available providers.

#### /usage

/usage \[session\|global\]

Shows usage and cost information.

#### /config

/config validate

Checks whether the config is valid.

/config path

Gets the path of the configuration files being used.

/config show

Shows the serialized current configuration being used (with merged
parameters).

#### /exit

/exit

/quit

/q

Exit the interactive session.

Before exiting the session, it is persisted.

### Path references

The CLI must support path references using @.

\@path/to/file

The CLI should provide auto-complete for paths prefixed with @.

## JavaScript SDK

Smista.ai should provide a JavaScript/TypeScript SDK for interacting
with smista-router.

The SDK is a client for the HTTP API. It must not reimplement routing
logic, policy evaluation, provider selection, or tool mediation.

Core behaviour remains owned by smista-router.

### Purpose

The JavaScript SDK should allow external tools, scripts, editors, and
future web clients to interact with smista.ai programmatically.

The SDK should support:

- Authentication

- Session creation and management

- Task execution

- Streaming responses

- Route preview

- Trace inspection

- Usage inspection

- Provider/model listing

### API client

The SDK should expose a typed client.

const client = **new** *SmistaClient*({

routerUrl: \"http://127.0.0.1:7331\",

token: process.env.SMISTA_TOKEN,

});

Example usage:

const session = await client.sessions.*create*({

title: \"Refactor auth middleware\",

});

const result = await client.sessions.*execute*(session.id, {

input: {

text: \"refactor the auth middleware\",

command: \"edit\",

},

});

### Types

The SDK should use TypeScript types aligned with the router API.

Where practical, SDK types should be generated from shared Rust
API/domain types or an OpenAPI schema.

The SDK must avoid maintaining independent copies of core routing and
policy models when generation is possible.

### Package

The SDK package may be published as:

\@smista-ai/sdk

## Installation

Smista.ai must provide simple installation scripts for macOS, Linux, and
Windows.

The official installation entrypoint should be hosted at:

https://install.smista.ai

The installation script must detect the operating system and CPU
architecture, download the correct release artifact, verify it where
possible, and install the selected binaries into the configured binary
directory.

### Supported platforms

mista.ai must distribute prebuilt binaries for:

- macOS x86_64

- macOS aarch64

- Linux x86_64

- Linux aarch64

- Windows x86_64

- Windows aarch64

Linux binaries should be statically linked with musl where practical.

macOS and Windows binaries should use the native platform targets.

### Install scripts

Unix-like systems should support installation through a shell script:

curl -fsSL https://install.smista.ai \| sh

Windows should support installation through PowerShell:

irm https://install.smista.ai/install.ps1 \| iex

The scripts must support both interactive and non-interactive usage.

### Components

The installer must allow the user to install:

- smista-cli

- smista-router

- both components

In interactive mode, the installer should prompt the user to choose
which components to install.

In non-interactive mode, the user must be able to pass flags.

Examples:

curl -fsSL https://install.smista.ai \| sh -s \-- \--cli\
curl -fsSL https://install.smista.ai \| sh -s \-- \--router\
curl -fsSL https://install.smista.ai \| sh -s \-- \--cli \--router

PowerShell equivalent:

irm https://install.smista.ai/install.ps1 \| iex\
install-smista \--cli \--router

### Installer flags

The installer script should support these flags:

\--cli\
\--router\
\--version \<version\>\
\--bin-dir \<path\>\
\--yes\
\--no-prompt\
\--no-brew\
\--force\
\--print-only\
\--help

Where:

- \--cli installs smista-cli

- \--router installs smista-router

- \--version installs a specific version

- \--bin-dir selects the installation directory

- \--yes accepts prompts using defaults

- \--no-prompt disables interactive prompts

- \--no-brew disables Homebrew usage on macOS

- \--force overwrites an existing installation

- \--print-only prints the planned actions without installing

- \--help shows installer help

If neither \--cli nor \--router is provided in non-interactive mode, the
installer should install both components by default and fail with an
actionable error. The preferred default is to install both.

### Binary installation

Default UNIX installation paths may include:

\~/.local/bin

/usr/local/bin

Default Windows installation paths may include:

\$LOCALAPPDATA%\\smista\\bin

### Post-installation behaviour

After installation, the installer should print:

- Installed components

- Installed version

- Binary path

- Whether the path is available in PATH

- Next suggested command

### PATH setup

If the installed location is not in PATH after installation, on POSIX
systems it should be added to PATH by updating the config.fish, bashrc,
zshrc, etc.
