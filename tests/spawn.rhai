
print(`Main`);

for arg in ARG {
	print(`arg: ${arg}`);
}

// Create users in background.
let users_handle = ENGINE.spawn_file_task_args("./scripts/create_users.rhai", ARG);
print("spawned 'Create users' task.");

// Create another task.
let handle = ENGINE.spawn_task_args(`
	print("From sub task.");
	42
`, ARG);

print("spawned child task.");
let res = handle.join();
print(`joined.  res = ${res}`);

let res = users_handle.join();
print(`users res = ${res}`);
