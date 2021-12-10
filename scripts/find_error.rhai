let mod_idx = parse_int(ARG.shift());
let err_idx = parse_int(ARG.shift());

let error = METADATA.find_error(mod_idx, err_idx);
print(`error = ${error}`);
