// Try mocking a CDD registration.

print("TestUtils: Mock CDD");

// Use Alice for mocking cdd.
let alice = USER.Alice;

let api = alice.connect("ws://127.0.0.1:9944");

// Test user.
let user = USER.Test123;

let func = METADATA.TestUtils.mock_cdd_register_did;
let encoded = func.encode_call([user]);
print(`mock_cdd.encoded: ${encoded}, len: ${encoded.len()}`);

let hash = api.submit_call(encoded);
print(`hash = ${hash}`);

