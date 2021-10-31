// simple test.

print("Test Client API");

// Test creating users.
let user = USER.Alice;
print(`User: ${user}`);

//let api = connect("ws://127.0.0.1:9944");
let api = user.connect("ws://127.0.0.1:9944");
//api.print_metadata();

let pallet = METADATA.Balances;
print(`pallet: ${pallet}`);

let storage = pallet.TotalIssuance;
print(`storage: ${storage}`);
//print(`storage.title: ${storage.title}`);

let func = pallet.transfer;
print(`func: ${func}`);

//print(`func.title: ${func.title}`);

//print(`func.docs: ${func.docs}`);

//*
for mod in METADATA.modules {
	print(`module: ${mod}`);
	for func in mod.funcs {
		print(`  ${func}`);
		for arg in func.args {
			print(` -- ${arg}`);
			//let res = Types.parse_type(arg.type());
			//print(`   -- ${res}`);
		}
	}
	print()
}
//*/

let func = METADATA.Relayer.set_paying_key;
print(`Relayer: ${func}, ${func.args.len()}`);
for arg in func.args {
	print(` -- ${arg}`);
	let res = Types.parse_type(arg.type());
	print(`type: ${res}`);
}

Types.dump_types();
//Types.dump_unresolved();

/*
let res = Types.parse_type("Option<Moment>");
print(`type: ${res}`);

let res = Types.parse_type("T::AccountId");
print(`type: ${res}`);
*/

