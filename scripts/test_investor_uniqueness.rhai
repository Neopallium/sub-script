// Test Investor Uniqueness claims.
print("Identity: Test add investor Uniqueness claim");

// Use Alice for mocking cdd.
let alice = USER.Alice;

// Test user.
let user = USER.Test123;

// Mock Cdd for user and make sure they have POLYX.
let res = alice.submit(TestUtils.mock_cdd_register_did(user));
if res.is_success {
	alice.submit(Balances.transfer(user, 5.0));
}

// Create investor uniqueness claim & proof.
let claim = PolymeshUtils.create_investor_uniqueness(user, "ACME");
print(`claim = ${claim}`);

// Add Investor Uniqueness claim.
let res = user.submit(Identity.add_investor_uniqueness_claim(user, claim[0], claim[1], ()));
print(`Add investor uniqueness: ${res}`);
