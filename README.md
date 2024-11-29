# nomos-node
Nomos blockchain node mvp


## Project structure

- `nomos-core`: Nomos core is the collection of essential structures for the Nomos mvp and experimental nodes.
- `nomos-services`: Nomos services is the collection of components that are used as building blocks of the Nomos node prototype and the experimental nodes.
  - consensus
  - log
  - http
  - mempool
  - network
- `nodes`: Nomos nodes is the collection of nodes that are used to run the Nomos mvp and experimental nodes.
  - `nomos-node`: main implementation of the Nomos mvp node.
  - `mockpool-node`: node with single mempool service, used to measure transaction dissemination.


## Services

The Nomos node uses `Overwatch` as its main internal framework. This means that services request communication channels 
to other services to interchange information through a specified messaging API.

### Service architecture

Most of the services are implemented with the same idea behind. There is a front layer responsible for handling the `Overwatch` service
and a back layer that implements the actual service logic.

This allows us to easily replace components as needed in a type level system. In any case, a node can be setup in a declarative way composing the types.
For example:

```rust
...
#[derive(Services)]
struct MockPoolNode {
    logging: ServiceHandle<Logger>,
    network: ServiceHandle<NetworkService<Waku>>,
    mockpool: ServiceHandle<MempoolService<WakuAdapter<Tx>, MockPool<TxId, Tx>>>,
    http: ServiceHandle<HttpService<AxumBackend>>,
    bridges: ServiceHandle<HttpBridgeService>,
}
```



## Docker

To build and run a docker container with Nomos node run:

```bash
docker build -t nomos .
docker run nomos /etc/nomos/config.yml
```

To run a node with a different configuration run:

```bash
docker run -v /path/to/config.yml:/etc/nomos/config.yml nomos /etc/nomos/config.yml
```
