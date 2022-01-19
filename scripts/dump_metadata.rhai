// Dump node metadata.

let modules = METADATA.modules;
modules.sort(|m1, m2| {
	return m1.index - m2.index;
});
for mod in modules {
	let name = mod.name;
	let idx = mod.index;
	print(`-- Pallet: ${idx} -> ${name}`);
	let funcs = mod.funcs;
	funcs.sort(|f1, f2| {
		return f1.index - f2.index;
	});
	for func in funcs {
		let name = func.name;
		let idx = func.index;
		let args = func.args.map(|t| t.type);
		print(`  Func: ${idx} -> ${name}(${args})`);
	}
	let storage = mod.storage;
	storage.sort(|s1, s2| {
		let s1 = s1.name;
		let s2 = s2.name;
		if s1 == s2 {
			return 0;
		} else if s1 < s2 {
			return -1;
		} else {
			return 1;
		}
	});
	for storage in storage {
		print(`  Storage: ${storage.name}: map_keys=${storage.hasher_name}, value=${storage.value_type_name}`);
	}
}
