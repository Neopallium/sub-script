
let era_ty = Types.resolve("Era");
print(`era_ty = ${era_ty}`);

let era = era_immortal();
print(`immortal = ${era}`);
let enc = era_ty.encode(era);
print(`enc = ${enc}`);
let dec = era_ty.decode(enc);
print(`dec = ${dec}`);

let era = era_mortal(1, 10);
print(`mortal = ${era}`);
//let enc = era_ty.encode(era);
let enc = era.encode();
print(`enc = ${enc}`);
let dec = era_ty.decode(enc);
print(`dec = ${dec}`);
