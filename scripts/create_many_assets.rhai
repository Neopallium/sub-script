// Create a large number of assets.

print("Batching asset creation.");

// Use Alice for mocking cdd.
let alice = USER.Alice;

// Asset owner user.
let user = USER.Test123;

// Mock Cdd for user and make sure they have a lot of POLYX.
alice.submit(TestUtils.mock_cdd_register_did(user));
alice.submit(Balances.transfer(user, 1000000.0));

let calls = [];
for id in range(0, 100) {
	calls += Asset.create_asset(
		`Test: ${id}`,
		`T-${id}`,
		true,
		#{ Custom: 1000 + id },
		[],
		(),
		false
	);
}


let batch = Utility.batch_optimistic(calls);
//print(`encoded batch: ${batch}`);
let res = alice.submit(batch);
print(`batch result: ${res.result}`);

