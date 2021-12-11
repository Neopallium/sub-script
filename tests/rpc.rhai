import "../scripts/mod/polymesh" as p;

let client = RPC.get_client("ws://127.0.0.1:9944");
print(`client = ${client}`);

client.call_method("system_properties",[]);
client.call_method("system_chain",[]);
client.call_method("system_chainType",[]);
client.call_method("system_name",[]);
client.call_method("system_version",[]);

client.subscribe("state_subscribeStorage",[
	["0x5f3e4907f716ac89b6347d15ececedca308ce9615de0775a82f8a94dc3d285a1"]
], "state_unsubscribeStorage");

client.subscribe("state_subscribeStorage",[
	"0x5f3e4907f716ac89b6347d15ececedca308ce9615de0775a82f8a94dc3d285a1"
], "state_unsubscribeStorage");

// Test user.
let user = USER.Test123;

// Make sure the user has an Identity.
p::mock_user(user, 20.0);

let val = STORAGE.map("Identity", "KeyToIdentityIds", user.acc);
print(`did = ${val}`);
