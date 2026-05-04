# PSKit

PSKit is a collection of tools and services for bioinformatics / biomedical workflows, providing a web UI and a task execution backend.

- Website: https://pskit.bioailab.net

## Run

Build the image:

```bash
docker build -t pskit .
```

Run the container (mount model parameters and the tasks workspace as needed):

```bash
docker run -d \
  --name pskit \
  --restart unless-stopped \
  -p 127.0.0.1:10706:10706 \
  -v <model_parameters_dir>:/app/pskit/ai/model_parameters \
  -v <tasks_dir>:/app/tasks \
  -e API_KEY=XXX
  pskit
```

## Project Structure

- `Dockerfile`: builds Rust binaries (web server + WASM), builds the frontend, and assembles the runtime image.
- `pskit/ai/`: Python task logic (feature extraction, inference, task runner).
- `pskit/toolkit/`: Rust workspace with shared crates, including the WASM crate used by the frontend.
- `pskit/pskit-wasm-pkg/`: generated WASM JS/TS package output (from `wasm-bindgen`) used by the frontend.
- `webserver/`: Rust web server binary (`pskit-webserver`) that serves the API and the built frontend.
- `webpage/`: frontend (Vite) source code and build output (`dist/`).
- `tasks/`: runtime task workspace; typically contains inputs, outputs, and `results/` folders.
