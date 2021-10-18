// simple test.

print("Test Client API");

// Test creating users.
let user = USER.Alice;
print(`User: ${user}`);

//let api = connect("ws://127.0.0.1:9944");
let api = user.connect("ws://127.0.0.1:9944");
//api.print_metadata();

let pallet = API.Balances;
print(`pallet: ${pallet}`);

let storage = pallet.TotalIssuance;
print(`storage: ${storage}`);
//print(`storage.title: ${storage.title}`);

let func = pallet.transfer;
print(`func: ${func}`);

//print(`func.title: ${func.title}`);

//print(`func.docs: ${func.docs}`);

for mod in API.modules {
	print(`module: ${mod}`);
	for func in mod.funcs {
		print(`  ${func}`);
	}
	print()
}

