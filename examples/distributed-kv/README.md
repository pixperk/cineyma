# distributed key-value store

a simple distributed key-value store demonstrating cineyma's cluster capabilities.

## features

- **cluster membership**: nodes discover each other via gossip protocol
- **actor discovery**: automatic actor location tracking across nodes
- **remote messaging**: location-transparent communication with actors
- **interactive cli**: simple interface for kv operations

## architecture

each node runs:
- a `KVStore` actor that handles get/set/delete operations
- a cluster server that handles both gossip and actor messages
- periodic gossip to share membership and actor locations
- a cli for user interaction

## running the example

### start first node

```bash
cargo run -p distributed-kv -- node-1
```

### start second node (joins via first)

```bash
cargo run -p distributed-kv -- node-2 127.0.0.1:7001
```

### start third node

```bash
cargo run -p distributed-kv -- node-3 127.0.0.1:7001
```

## commands

```
get <key>           - retrieve value for key
set <key> <value>   - store key-value pair
delete <key>        - remove key
members             - list cluster members
actors              - show registered actors
quit                - exit node
```

## example session

```
> set user:1 alice
OK

> get user:1
alice

> members
cluster members:
  node-1 @ 127.0.0.1:7001 [Up]
  node-2 @ 127.0.0.1:7002 [Up]

> actors
registered actors:
  kv-store -> node-1 (KVStore)

> delete user:1
deleted

> get user:1
(nil)
```

## what this demonstrates

1. **cluster discovery**: nodes automatically discover each other
2. **actor registry**: `kv-store` actor location is gossiped across cluster
3. **location transparency**: any node can send messages to `kv-store` without knowing which node it's on
4. **typed api**: uses `ClusterRemoteAddr::call()` for type-safe remote calls
5. **failure detection**: nodes track member health and clean up actors from failed nodes

## implementation details

- **ports**: auto-assigned as 7000 + node number (node-1 → 7001)
- **protocol**: single port multiplexes gossip and actor messages
- **storage**: simple in-memory hashmap (not replicated)
- **discovery**: gossip spreads membership every 2 seconds
- **failure detection**: nodes marked suspect after 10s, down after 20s
