print("Create a JoinIdentity authorization.");

// Use Alice for mocking cdd.
let alice = USER.Alice;

// Test users
let user = USER.Test123;
let key = USER.Key1;

// Mock Cdd for user and make sure they have POLYX.
let res = alice.submit(TestUtils.mock_cdd_register_did(user));
if res.is_success {
	alice.submit(Balances.transfer(user, 5.0));
}

// Add JoinIdentity authorization for `key` to join `user`.
let res = user.submit(Identity.add_authorization(#{
	Account: key
}, #{
	JoinIdentity: #{
		asset: #{ These: ["ACME"] },
		//asset: #{ Whole: () },
		extrinsic: #{ Whole: () },
		portfolio: #{ Whole: () },
	}
}, ()));
let events = res.events("Identity.AuthorizationAdded");

// Get auth_id from events and remove the authorization.
for event in events {
	let auth_id = event.args[3];
	print(`${event.name}: auth_id=${auth_id}`);
	key.submit(Identity.remove_authorization(#{Account: key }, auth_id, true));
}

/*
let removed = 0;
// Try removing auth_ids 100 to 150 for `key`.
for auth_id in range(10, 100) {
	try {
		let res = key.submit(
			Identity.remove_authorization(#{Account: key }, auth_id, true)
		);
		if res.is_success {
			print(`removed auth_id=${auth_id}`);
		}
		removed += 1;
	} catch(err) {
		//print(err);
	}
}
print(`Removed ${removed} authorizations`);
// */
