```bash
docker build -t pskit .
docker run -d \
  --name pskit \
  --restart unless-stopped \
  -p 127.0.0.1:10706:10706 \
  -v <model_parameters_dir>:/app/pskit/ai/model_parameters \
  -v <lib_dir>:/app/pskit/ai/lib \
  -v <tasks_dir>:/app/tasks \
  pskit
```
