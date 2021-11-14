// Try mocking a CDD registration.

print("TestUtils: Mock CDD");

// Use Alice for mocking cdd.
let alice = USER.Alice;

// Test user.
let user = USER.Test123;

fn dump_func(func) {
	print(func.to_string());
	for arg in func.args {
		print(" -- " + arg.to_string());
	}
}
//dump_func(METADATA.TestUtils.mock_cdd_register_did);
let encoded = METADATA.TestUtils.mock_cdd_register_did.encode_call(user);
print(`mock_cdd.encoded: ${encoded}, len: ${encoded.len()}`);
let hash = alice.submit_call(encoded);
print(`hash = ${hash}`);

//dump_func(METADATA.Balances.transfer);
//*
//let encoded = METADATA.Balances.transfer.encode_call(user, 1);
//print(`transfer.encoded: ${encoded}, len: ${encoded.len()}`);
let encoded = METADATA.Balances.transfer.encode_call(user, 15.0);
print(`transfer.encoded: ${encoded}, len: ${encoded.len()}`);
let hash = alice.submit_call(encoded);
print(`hash = ${hash}`);
// */

//let encoded = METADATA.TestUtils.mock_cdd_register_did.encode_call(user);
let encoded = METADATA.Balances.transfer.encode_call(alice, 1.2);
print(`transfer.encoded: ${encoded}, len: ${encoded.len()}`);
let hash = user.submit_call(encoded);
print(`hash = ${hash}`);

