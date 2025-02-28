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
    logging: OpaqueServiceHandle<Logger>,
    network: OpaqueServiceHandle<NetworkService<Waku>>,
    mockpool: OpaqueServiceHandle<MempoolService<WakuAdapter<Tx>, MockPool<TxId, Tx>>>,
    http: OpaqueServiceHandle<HttpService<AxumBackend>>,
    bridges: OpaqueServiceHandle<HttpBridgeService>,
}
```



## Docker

To build and run a docker container with the Nomos node you need to mount both `config.yml` and `global_params_path` specified in the configuration. 

```bash
docker build -t nomos .

docker run -v "/path/to/config.yml" -v "/path/to/global_params:global/params/path" nomos /etc/nomos/config.yml

```

To use an example configuration located at `nodes/nomos-node/config.yaml`, first run the test that generates the random kzgrs file and then run the docker container with the appropriate config and global params:

```bash
cargo test --package kzgrs-backend write_random_kzgrs_params_to_file -- --ignored

docker run -v "$(pwd)/nodes/nomos-node/config.yaml:/etc/nomos/config.yml" -v "$(pwd)/nomos-da/kzgrs-backend/kzgrs_test_params:/app/tests/kzgrs/kzgrs_test_params" nomos /etc/nomos/config.yml

```


## License

This project is primarily distributed under the terms defined by either the MIT license or the
Apache License (Version 2.0), at your option.

See [LICENSE-APACHE](LICENSE-APACHE2.0) and [LICENSE-MIT](LICENSE-MIT) for details.