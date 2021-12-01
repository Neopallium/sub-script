
/*
let error = METADATA.find_error(19, 1);
print(`error = ${error}`);
*/

let idx_sort = |m1, m2| {
	let idx1 = m1.index;
	let idx2 = m2.index;
	if idx1 == idx2 {
		return 0;
	}
	if idx1 < idx2 {
		return -1;
	}
	if idx1 > idx2 {
		return 1;
	}
};

let mod_list = METADATA.modules;
mod_list.sort(idx_sort);

print("{");
print(`  "InvalidModuleError": {`);
print(`    "_enum": [ "Invalid" ]`);
print(`  },`);
print(`  "ModuleNoErrors": {`);
print(`    "_enum": [ "None" ]`);
print(`  },`);

for mod in mod_list {
	let err_list = mod.errors;
	let err_len = err_list.len();
	if err_len == 0 {
		continue;
	}

	err_list.sort(idx_sort);
	let mod_name = mod.name;
	print(`  "${mod_name}Error": {`);
	print(`    "_enum": [`);
	for (err, idx) in err_list {
		let err_name = err.name;
		if (idx + 1) < err_len {
			print(`      "${err_name}",`);
		} else {
			print(`      "${err_name}"`);
		}
	}
	print(`    ]`);
	print(`  },`);
}

let next_mod_idx = 0;
print(`  "DispatchErrorModule": {`);
print(`    "_enum": {`);
let last_mod_idx = mod_list.len() - 1;
for mod in mod_list {
	let mod_idx = mod.index;
	let mod_name = mod.name;

	// Detect skipped module index.
	while next_mod_idx < mod_idx {
		print(`      "InvalidModule${next_mod_idx}": "InvalidModuleError",`);
		next_mod_idx += 1;
	}

	let err_ty = `${mod_name}Error`;

	if mod.errors.len() == 0 {
		err_ty = "ModuleNoErrors";
	}

	if mod_idx < last_mod_idx {
		print(`      "${mod_name}": "${err_ty}",`);
	} else {
		print(`      "${mod_name}": "${err_ty}"`);
	}

	next_mod_idx += 1;
}
print(`    }`);
print(`  }`);

print("}");
