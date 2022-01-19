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
		print(`  Func: ${idx} -> ${name}`);
	}
}
