# Docker Compose Testnet for Nomos

The Nomos Docker Compose Testnet contains four distinct service types:

- **Nomos Node Services**: Multiple dynamically spawned Nomos nodes that synchronizes their configuration via cfgsync utility.

## Building

Upon making modifications to the codebase or the Dockerfile, the Nomos images must be rebuilt:

```bash
docker compose build
```

## Configuring

Configuration of the Docker testnet is accomplished using the `.env` file. An example configuration can be found in `.env.example`.

To adjust the count of Nomos nodes, modify the variable:

```bash
DOCKER_COMPOSE_LIBP2P_REPLICAS=100
```

## Running

Initiate the testnet by executing the following command:

```bash
docker compose up
```

This command will merge all output logs and display them in Stdout. For a more refined output, it's recommended to first run:

```bash
docker compose up -d
```

Followed by:

```bash
docker compose logs -f nomos-node
```

## Using testnet

Bootstrap node is accessible from the host via `3000` and `18080` ports. To expose other nomos nodes, please update `nomos-node` service in the `compose.yml` file with this configuration:

```bash
  nomos-node-0:
    ports:
    - "3001-3010:3000" # Use range depending on the number of nomos node replicas.
    - "18081-18190:18080"
```

After running `docker compose up`, the randomly assigned ports can be viewed with `ps` command:

```bash
docker compose ps 
```
