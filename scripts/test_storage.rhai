import "polymesh" as p;

// Test user.
let user = USER.Test123;

// Make sure the user has an Identity.
p::mock_user(user, 20.0);

let val = STORAGE.map("Identity", "KeyToIdentityIds", user.acc);
print(`did = ${val}`);
