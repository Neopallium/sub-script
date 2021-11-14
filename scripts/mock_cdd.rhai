// Try mocking a CDD registration.

print("TestUtils: Mock CDD");

// Use Alice for mocking cdd.
let alice = USER.Alice;

// Test user.
let user = USER.Test123;

let encoded = TestUtils.mock_cdd_register_did(user);
print(`mock_cdd.encoded: ${encoded}, len: ${encoded.len()}`);
let hash = alice.submit(encoded);
print(`hash = ${hash}`);

//*
//let encoded = Balances.transfer(user, 1);
let encoded = Balances.transfer(user, 15.0);
print(`transfer.encoded: ${encoded}, len: ${encoded.len()}`);
let hash = alice.submit(encoded);
print(`hash = ${hash}`);
// */

let encoded = Balances.transfer(alice, 1.2);
print(`transfer.encoded: ${encoded}, len: ${encoded.len()}`);
let hash = user.submit(encoded);
print(`hash = ${hash}`);

