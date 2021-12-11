import "../scripts/mod/polymesh" as p;

let client = RPC.get_client("ws://127.0.0.1:9944");
print(`client = ${client}`);

let tokens = [];

tokens += client.call_method("system_properties",[]);
tokens += client.call_method("system_chain",[]);
tokens += client.call_method("system_chainType",[]);
tokens += client.call_method("system_name",[]);
tokens += client.call_method("system_version",[]);

let sub = client.subscribe("state_subscribeStorage",[
	["0x5f3e4907f716ac89b6347d15ececedca308ce9615de0775a82f8a94dc3d285a1"]
], "state_unsubscribeStorage");
tokens += sub;

tokens += client.subscribe("state_subscribeStorage",[
	"0x5f3e4907f716ac89b6347d15ececedca308ce9615de0775a82f8a94dc3d285a1"
], "state_unsubscribeStorage");

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
