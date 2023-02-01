# nomos-research
Nomos blockchain node mvp

## Docker

To build and run a docker container with nomos node run:

```bash
docker build -t nomos .
docker run nomos /etc/nomos/config.yml
```

To run a node with a different configuration run:

```bash
docker run -v /path/to/config.yml:/etc/nomos/config.yml nomos /etc/nomos/config.yml
```
