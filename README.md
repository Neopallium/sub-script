# Sub-script

A simple scripting interface for [Substrate](https://substrate.dev) nodes.
It uses [Rhai](https://rhai.rs) for the scripting engine.

## Features

- Auto generate key pairs for named users by accessing `USER.<UserName>`.
- Loads chain metadata for all modules, extrinsics, events and storages.
- Easy to sign and submit extrinsic calls.

## Goals

- Flexible - Load chain metadata from node and all custom types from `schema.json` file.  No recompile needed.
- Mocking - Easy to share scripts for mocking large number of users, assets, etc.. on a local node for testing UIs.
- Debuging - Provides a low-level interface to the node.  Easy to change schema types for testing encoding issues.

## Non-Goals

- This doesn't replace SDKs for other languages.  The available libraries will be very limited.
- Please don't use on Mainnet.

## Todo

- Subscribe to storage updates.
- Get storage values.
- Better interface for getting events.
- Replace/fix current `substrate-api-client` crate.
- Use a shared pool of client/connects to node.  (right now each user has a client connection)
- Add hook to auto-initialize new users (to support mocking cdd registration).
- Add REPL support for quickly making extrinsic calls.
- Support multiple `Engine` instances with shared User state (for nonces).  This will allow spawning sub-tasks for load testing.

## Examples

```rhai
// Use Alice for mocking cdd.
let alice = USER.Alice;

// Generate a test user.  Key generated from "//Test123" seed.
let user = USER.Test123;

// Mock Cdd for user and make sure they have some POLYX.
alice.submit(TestUtils.mock_cdd_register_did(user));
alice.submit(Balances.transfer(user, 5.0));

// Generate another test user.  Key generated from "//Key1" seed.
let key = USER.Key1; // Don't mock cdd for this user.

// Add JoinIdentity authorization for `key` to join `user`.
let block = user.submit(Identity.add_authorization(#{
	Account: key
}, #{
	JoinIdentity: #{
		asset: #{ These: ["ACME"] },
		extrinsic: #{ Whole: () },
		portfolio: #{ Whole: () },
	}
}, ()));
// Print block hash.
print(`block = ${block}`);
```

See other examples scripts in `./scripts/` folder.

