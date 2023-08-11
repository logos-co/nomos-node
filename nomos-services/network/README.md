# Nomos Network Service

Nomos Network Service is an Overwatch service, which runs the [`NetworkBackend`](./src/backends/mod.rs)
that processes messages from users and provides a function to subscribe for events from the network.

Currently, two `NetworkBackend` implementations are provided:
- [Waku](./src/backends/waku.rs)
- [Libp2p](./src/backends/libp2p.rs)


## Mixnet integration

Each `NetworkBackend` provides an option to enable the [`mixnet`](../../mixnet/) crate integration that provides the network-level privacy.

![](./docs/mixnet.drawio.png)

NOTE: Currently, this diagram is based on "broadcasting" at the last stage of the mixnet.
But, we are thinking about introducing the "private routing" mechanism instead of broadcasting,
in order to reduce the messaging complexity, even though it's a hard problem.
Also, this topic is outside the scope of mixnet integration.
Please note that we will keep using broadcasting for some message types, such as proposing blocks to the entire network.


### Purposes

Protection of the message sender identities from the network (from global adversaries, who can also be a part of the network)

This can be achieved by the following techniques[^1].
> 1. make all traffic look the same (so adversaries can’t follow individual traffic)
> 2. re-order traffic (so that adversaries can’t simply count what goes in and what comes out to statistically de-anonymise users)
> 3. generate fake cover traffic (so adversaries can’t know when anonymity was important)
 
### Design

- Mixnet integration strategy
  - The `mixnet` crate is imported into nomos-node (especially into network-backend), instead of having separated mixnode processs (possibly in different machines)
- Building a mix route
  - A mix route is built randomly from the mixnet topology.
  - The mixnet topology contains the IP address and public key of all mixnodes, and shared with all mixnodes in the network.
    - The mixnode public key shouldn't be the same as the consensus public key (associated with its `NodeId` or stake).
- Building Sphinx packets
  - A message is splitted into static-sized [Sphinx](https://cypherpunks.ca/~iang/pubs/Sphinx_Oakland09.pdf) packets.
  - Each packet is encapsulated with encryption using a public key of each mixnode in the route and delays (randomly chosen for now).
- Broadcasting the message after mixnet
  - A destination mixnode reconstructs the message by gathering all associated Sphinx packets, and broadcasts the message (via libp2p gossipsub).
  - As we mentioned [above](#mixnet-integration), we're thinking about better ways than broadcasting.


### TODOs

- Better mixnet topology management
    - Currently, the mixnet topology is configured in the config file, and is updated via advertisements from other mixnodes through libp2p gossipsub.
- "Layered" mixnet topology, for mixnodes to not establish connections with all other mixnodes
- Better mixnet transport between mixnodes (e.g. TCP conn pooling, multiplexing, or any other transport rather than TCP)
- Cover traffic (dummy traffic) for unobservability


[^1]: A simple introduction to mixnets: https://constructiveproof.com/posts/2020-02-17-a-simple-introduction-to-mixnets/
