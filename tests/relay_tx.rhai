import "../scripts/mod/polymesh" as p;

print("Test relay_tx");

// Use Alice for mocking cdd.
let alice = USER.Alice;

// Test users
let user1 = USER.Test123;
let user2 = USER.Test2;
p::mock_user(user1, 20.0);
p::mock_user(user2, 1.0);

let relay_call = p::make_relay_tx(user1, Balances.transfer(user2, 5.0));

let res = user2.submit(relay_call);
print(`res = ${res}`);
