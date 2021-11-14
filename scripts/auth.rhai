// Try batching mock cdd registrations.

print("Batching: Mock CDD");

// Use Alice for mocking cdd.
let alice = USER.Alice;

// Test users
let user = USER.Test123;
let key = USER.Key1;

//*
// Mock Cdd for user and make sure they have POLYX.
alice.submit(TestUtils.mock_cdd_register_did(user));
alice.submit(Balances.transfer(user, 5.0));
// */

//*
// Add JoinIdentity authorization for `key` to join `user`.
let block = user.submit(Identity.add_authorization(#{
	Account: key
}, #{
	JoinIdentity: #{
		asset: #{ These: ["ACME"] },
		extrinsic: #{ Whole: () },
		portfolio: #{ Whole: () },
	}
}, ()));
print(`block = ${block}`);
// */

//*
let removed = 0;
// Try removing auth_ids 100 to 150 for `key`.
for auth_id in range(100, 150) {
	try {
		//print(`remove auth: ${auth_id}`);
		let block = key.submit(
			Identity.remove_authorization(#{Account: key }, auth_id, true)
		);
		print(`block = ${block}`);
		removed += 1;
	} catch(err) {
		//print(err);
	}
}
print(`Removed ${removed} authorizations`);

// */
