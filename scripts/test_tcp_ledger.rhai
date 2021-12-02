print("Test simple transaction with Ledger app");

// Use Alice for mocking cdd.
let alice = USER.Alice;

// Polymesh ledger app
let app = LedgerApps.get_app("Polymesh: tcp:127.0.0.1:9999");
print(`ledger app: ${app}`);

//*
alice.submit(TestUtils.mock_cdd_register_did(app));
let res = alice.submit(Balances.transfer(app, 2.0));
print(`call result = ${res}`);
//print(`events = ${res.events}`);
// */

let res = app.submit(Balances.transfer(alice, 0.1));
print(`call result = ${res}`);
//print(`events = ${res.events}`);
