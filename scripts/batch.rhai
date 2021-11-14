// Try batching mock cdd registrations.

print("Batching: Mock CDD");

// Use Alice for mocking cdd.
let alice = USER.Alice;

// Test users
let users = [
	USER.Test123,
	USER.Test2,
	USER.Test3,
	USER.Test4,
	USER.Test5,
];

let api = alice.connect("ws://127.0.0.1:9944");

fn dump_func(func) {
	print(func.to_string());
	for arg in func.args {
		print(" -- " + arg.to_string());
	}
}

let calls = [];
for (user, idx) in users {
	calls += METADATA.TestUtils.mock_cdd_register_did.encode_call(user);
	calls += METADATA.Balances.transfer.encode_call(user, 5.0);
}

//dump_func(METADATA.Utility.batch_atomic);
let encoded = METADATA.Utility.batch_atomic.encode_call(calls);
print(`batchAtomic.encoded: ${encoded}, len: ${encoded.len()}`);
//*
let hash = alice.submit_call(encoded);
print(`hash = ${hash}`);
// */
