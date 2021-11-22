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

let res = alice.submit(Utility.batch_optimistic(calls));
//let res = alice.submit(Utility.batch_atomic(calls));

// Display all events for the batch.
for event in res.events {
	print(`EventName: ${event.name}`);
	print(`  Args: ${event.args}`);
}

// Display events match prefix 'Utility.Batch' for the batch.
for event in res.events("Utility.Batch") {
	print(`EventName: ${event.name}`);
	print(`  Args: ${event.args}`);
}

