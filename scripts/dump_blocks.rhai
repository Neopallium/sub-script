
//let call_ty = Types.resolve("Call");
//print(`call = ${call_ty}`);

// Only dump ImOnline.Heartbeat calls.
let filter = "041700";
//let filter = "04";

// Get arguments.
let block_number = ARG.shift();
while block_number != () {
	let block = CLIENT.get_block_by_number(parse_int(block_number));
	//print(`block = ${block}`);

	// Get hex encoded extrinsics from block.
	let calls = block.extrinsics_filtered(filter);
	print(`${block.block_number}: calls = ${calls}`);

	block_number = ARG.shift();
}
