# Architecture Overview

PhotoVault uses a single-process offline architecture:
- `src/app/` state machine and message handlers
- `src/views/` render tree per feature
- `src/services/` business logic and pipelines
- `src/db/` SQLite repositories and schema
- `src/ml/` ONNX inference wrappers
