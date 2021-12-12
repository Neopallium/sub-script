import "../scripts/mod/polymesh" as p;

let client = RPC_MANAGER.get_client("ws://127.0.0.1:9944");
print(`client = ${client}`);

let tokens = [];

tokens += client.async_method("chain_getBlockHash",[0]);
tokens += client.async_method("state_getRuntimeVersion",[]);
//tokens += client.async_method("state_getMetadata",[]);

tokens += client.async_method("system_chain",[]);
tokens += client.async_method("system_properties",[]);
tokens += client.async_method("system_chainType",[]);
tokens += client.async_method("system_name",[]);
tokens += client.async_method("system_version",[]);
tokens += client.async_method("chain_getBlockHash",[1000000]);

let sub = client.subscribe("state_subscribeStorage",[
	["0x5f3e4907f716ac89b6347d15ececedca308ce9615de0775a82f8a94dc3d285a1"]
], "state_unsubscribeStorage");
tokens += sub;

/*
// For testing rpc error.
tokens += client.subscribe("state_subscribeStorage",[
	"0x5f3e4907f716ac89b6347d15ececedca308ce9615de0775a82f8a94dc3d285a1"
], "state_unsubscribeStorage");
*/

//tokens.reverse();
for token in tokens {
	let resp = client.get_response(token);
	print(`resp = ${resp}`);
}
client.close_request(sub);

/*
// Test user.
let user = USER.Test123;

// Make sure the user has an Identity.
p::mock_user(user, 20.0);

let val = STORAGE.map("Identity", "KeyToIdentityIds", user.acc);
print(`did = ${val}`);
*/
