# Installation using Docker

## Building the Docker image

To build the Docker image, run the following command at the root of the project:
```bash
docker build -t nomos .
```

## Using the Docker image

You can run a Nomos node using the Docker image by running the following command:
```bash
docker run nomos /etc/nomos/config.yml
```

To run a node with a different configuration run:
```bash
docker run -v /path/to/config.yml:/etc/nomos/config.yml nomos /etc/nomos/config.yml
```

For more detailed instructions to configure and run Nomos nodes, refer to the [Run Nodes](../run-nodes.md).
