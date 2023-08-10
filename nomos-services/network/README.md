# Nomos Network Service

Nomos Network Service is an Overwatch service, which runs the [`NetworkBackend`](./src/backends/mod.rs)
that processes messages from users and provides a function to subscribe for events from the network.

Currently, two `NetworkBackend` implementations are provided:
- [Waku](./src/backends/waku.rs)
- [Libp2p](./src/backends/libp2p.rs)


## Mixnet integration

Each `NetworkBackend` provides an option to enable the [`mixnet`](../../mixnet/) crate integration that provides the network-level privacy.

![](./docs/mixnet.drawio.png)


### Design

- Integration strategy
  - The `mixnet` crate is imported into nomos-node (especially into network-backend), instead of having separated mixnode processs (possibly in different machines)
- Building a mix route
  - A mix route is built randomly from the mixnet topology.
  - The mixnet topology contains the IP address and public key of all mixnodes, and shared with all mixnodes in the network.
    - The mixnode public key shouldn't be the same as the consensus public key (associated with its `NodeId` or stake).
- Building Sphinx packets
  - A gossipsub message is splitted into static-sized [Sphinx](https://cypherpunks.ca/~iang/pubs/Sphinx_Oakland09.pdf) packets.
  - Each packet is encapsulated with encryption using a public key of each mixnode in the route and delays (randomly chosen for now).
- Gossipsub after mixnet
  - A destination mixnode rebuilds the message by gathering all associated Sphinx packets, and publishes the message via gossipsub.
  - Gossipsub is performed with the regular libp2p transport (e.g. TCP), without mixnet.


### Privacy considerations

The following privacy considerations must be reviewed in order to make sure that our approaches meet all privacy requirements.

#### Unlinkability

From the [Nym whitepaper](https://nymtech.net/nym-whitepaper.pdf):
> In an abstract sense, unlinkability refers to the inability to determine
> which pieces of data available at different parts of a system may or may not be related to each other.
> More concretely, consider user identities at the client side (e.g., IP addresses, public keys, device identifiers)
> and messages, accesses, or transactions at the service side. 

Does the current design achieve
- unlinkability between consensus node identities (associated with `NodeId` or stake) and messages?
- unlinkability between messages?

#### Unobservability

From the [Nym whitepaper](https://nymtech.net/nym-whitepaper.pdf):
> ... while unobservability means that the adversary cannot even determine whether the user is sending any
> message at all, or just being idle. Unobservability conceals the activity patterns of users and adds idle
> users to the anonymity set. Unobservability is achieved through the use of "cover" (or "dummy") traffic ...

We may need to adopt cover (dummy) traffic modules from [Nym](https://github.com/nymtech/nym).


### Technical considerations

- Better mixnet topology management?
- Better mixnet transport between mixnodes, instead of establishing TCP conns every time
- Direct p2p message delivery instead of gossipsub, for some message types (such as `VoteMsg`)
  - Direct p2p messaging requires the `NodeId <> IPAddr` mapping.
    - For example, a certain node (or routing nodes) should know the IP address of `Node P` in its parent committee to send a `VoteMsg`.
    - But, we don't want to reveal the `NodeId <> IPAddr` mapping to anyone for the consensus node privacy. We need to investigate how we can achieve the direct p2p messaging without compromising the consensus node privacy.
  - This topic is not related with mixnet directly, but we need to think if we can leverage mixnet to solve this issue.
- TDB
