// Try batching mock cdd registrations.

print("Batching: Mock CDD");

// Use Alice for mocking cdd.
let alice = USER.Alice;

// Test users
let users = [
	USER.Test123,
	//*
	USER.Test2,
	USER.Test3,
	USER.Test4,
	USER.Test5,
	// */
];

let calls = [];
for (user, idx) in users {
	calls += TestUtils.mock_cdd_register_did(user);
	calls += Balances.transfer(user, 5.0);
}

//*
let block = alice.submit(Utility.batch_atomic(calls));
print(`block = ${block}`);
// */
