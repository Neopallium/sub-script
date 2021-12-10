import "../scripts/polymesh" as p;

// Test Investor Uniqueness claims.
print("Identity: Test add investor Uniqueness claim");

// Test user.
let user = USER.Test123;

p::mock_user(user, 20.0);

let res = p::add_investor_uniqueness_claim(user, "ACME");
print(`Add investor uniqueness: ${res}`);
