import "../scripts/mod/polymesh" as p;

// Test user.
let user = USER.Test123;

// Make sure the user has an Identity.
p::mock_user(user, 20.0);

let val = STORAGE.map("Identity", "KeyToIdentityIds", user.acc);
print(`did = ${val}`);

/*
// TODO: Add support for storage prefix lookups.
let val = STORAGE.double_map("Identity", "Authorizations", #{
		Account: user.acc
	}, ());
print(`auths = ${val}`);
*/

let val = STORAGE.map("System", "Account", user.acc);
print(`did = ${val}`);
