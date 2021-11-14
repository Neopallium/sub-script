// simple test.

print("Test Client API");

// Test creating users.
let user = USER.Alice;
print(`User: ${user}`);

let api = user.connect("ws://127.0.0.1:9944");
//api.print_metadata();

/*
let pallet = METADATA.Balances;
print(`pallet: ${pallet}`);

let storage = pallet.TotalIssuance;
print(`storage: ${storage}`);
//print(`storage.title: ${storage.title}`);

let func = pallet.transfer;
print(`func: ${func}`);
*/

//print(`func.title: ${func.title}`);

//print(`func.docs: ${func.docs}`);

/*
let max_args = 0;
for mod in METADATA.modules {
	for func in mod.funcs {
		let args = func.args.len();
		if args > max_args {
			max_args = args;
		}
		if args > 5 {
			print(`  ${func}`);
			print(`args = ${args}`);
		}
		/*
		print(`  ${func}`);
		for arg in func.args {
			print(` -- ${arg}`);
		}
		*/
	}
}
print(`max_args = ${max_args}`);
// */

/*
for mod in METADATA.modules {
	print(`${mod}`);
	for event in mod.events {
		print(`  ${event}`);
	}
	print()
}
// */

/*
for mod in METADATA.modules {
	print(`${mod}`);
	for storage in mod.storage {
		print(`  ${storage}`);
	}
	print()
}
// */

/*
let func = METADATA.Relayer.set_paying_key;
print(`Relayer: ${func}, ${func.args.len()}`);
for arg in func.args {
	print(` -- ${arg}`);
	let res = Types.parse_type(arg.type());
	print(`type: ${res}`);
}
// */

//Types.dump_types();
Types.dump_unresolved();

/*
let res = Types.parse_type("Option<Moment>");
print(`type: ${res}`);

let res = Types.parse_type("T::AccountId");
print(`type: ${res}`);
*/

/*
let func = METADATA.TestUtils.mock_cdd_register_did;
print(`mock_cdd: ${func}, ${func.args.len()}`);
for arg in func.args {
	print(` -- ${arg}`);
	let res = Types.parse_type(arg.type());
	print(`type: ${res}`);
}
let encoded = func.encode_call([user]);
print(`mock_cdd.encoded: ${encoded}, len: ${encoded.len()}`);
// */

let ty = Types.resolve("Call");
print(`Call: ${ty}`);

