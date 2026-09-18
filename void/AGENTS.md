# Project Context

This is a general-purpose workspace for code agents, organized into project- or topic-specific subdirectories as needed.

There is no application-specific codebase, architecture, build system, test suite, or product context to assume by default.

Use this workspace when the user needs help with general tasks that do not belong to an existing project, such as:

- General Q&A and technical explanation
- Research and investigation
- Working with remote hosts or external systems
- Creating temporary scripts, notes, or prototypes
- Processing ad hoc files provided by the user

When working in this project:

- Do not infer a framework, language, package manager, or repository structure unless files in the workspace or the user explicitly indicate one.
- Prefer minimal, task-specific files over scaffolding a full project.
- Ask a concise clarifying question if the task requires project-specific assumptions.
- Treat generated files as disposable unless the user asks to preserve or organize them.
- Prefer using temporary directories or clearly named scratch files for experiments.
- Keep intermediate notes and temporary artifacts in clearly named files or directories.

## Task Directories

- At the start of each new user task, determine whether it requires working in a directory. Purely conversational tasks do not require a new directory.
- For tasks that need a directory, check the workspace's existing subdirectories and their `AGENTS.md` files for a matching project or work topic. Reuse a directory when the task fits its documented scope; sharing a broad domain alone is insufficient.
- If no relevant subdirectory exists, create one under the workspace root and work there.
- Use a concise name identifying the concrete project, tool, or reusable work topic. When working on a named repository or product, prefer its recognizable name: for example, deployment and maintenance of `OpenMOSS/MOSS-Transcribe-Diarize` belong in `moss-transcribe-diarize`. For work without a named project, use a focused topic such as `speaker-diarization` or `android-build-tools`.
- Choose a scope that keeps related setup, debugging, and maintenance together while separating unrelated projects. Avoid broad catch-all names such as `audio`, `ai`, or `research`, and omit one-off action details, dates, and ticket IDs from directory names.
- Maintain an `AGENTS.md` in each directory used for tasks. Create it if missing, and describe the directory's purpose, scope, and what belongs there. Update it as that purpose or scope evolves, preserving any existing relevant instructions.
- Keep task files and artifacts within the selected directory; use clearly named scratch files or temporary subdirectories there for experiments.
