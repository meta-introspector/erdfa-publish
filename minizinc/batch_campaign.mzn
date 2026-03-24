% eRDFa Batch Campaign Planner — Multi-File Distribution
% Assigns each file an ECC code and distributes across platforms.
% Uses real parameters from erdfa-publish plugin registry + EC Zoo.

include "globals.mzn";

%% ─── File Parameters (from .dzn) ───

int: num_files;
array[1..num_files] of int: file_bytes;
array[1..num_files] of string: file_name;

%% ─── Platform Parameters (from .dzn) ───

int: num_platforms;
array[1..num_platforms] of string: platform_name;
array[1..num_platforms] of int: capacity;       % bytes per carrier (after 8B header)
array[1..num_platforms] of int: max_carriers;   % max carriers per platform (global)
array[1..num_platforms] of int: cost_microusd;  % cost per carrier

%% ─── ECC Parameters (from EC Zoo / erdfa-publish) ───

int: num_eccs;
array[1..num_eccs] of string: ecc_name;
array[1..num_eccs] of int: ecc_n;              % codeword length
array[1..num_eccs] of int: ecc_k;              % data symbols
array[1..num_eccs] of int: ecc_d;              % min distance
array[1..num_eccs] of int: ecc_expansion_pct;  % expansion as percentage (100 = 1×, 200 = 2×)

%% ─── Global Constraints ───

int: max_total_carriers;
bool: devnet;  % true = all blockchain costs zeroed

%% ─── Decision Variables ───

% Per-file: which ECC to use
array[1..num_files] of var 1..num_eccs: file_ecc;

% Per-file × per-platform: how many carriers
array[1..num_files, 1..num_platforms] of var 0..100: alloc;

%% ─── Derived ───

% Protected size per file = file_bytes × ecc_expansion / 100
array[1..num_files] of var int: protected =
  [file_bytes[f] * ecc_expansion_pct[file_ecc[f]] div 100 | f in 1..num_files];

% Per-file total capacity
array[1..num_files] of var int: file_capacity =
  [sum(p in 1..num_platforms)(alloc[f,p] * capacity[p]) | f in 1..num_files];

% Per-file carrier count
array[1..num_files] of var int: file_carriers =
  [sum(p in 1..num_platforms)(alloc[f,p]) | f in 1..num_files];

%% ─── Constraints ───

% Each file's capacity must cover its protected payload
constraint forall(f in 1..num_files)(file_capacity[f] >= protected[f]);

% Per-platform global carrier limit
constraint forall(p in 1..num_platforms)(
  sum(f in 1..num_files)(alloc[f,p]) <= max_carriers[p]
);

% Total carriers within budget
constraint sum(f in 1..num_files)(file_carriers[f]) <= max_total_carriers;

% Each file uses at least 2 platforms (resilience)
constraint forall(f in 1..num_files)(
  count(p in 1..num_platforms)(alloc[f,p] > 0) >= 2
);

% Small files (< 1KB) must use strong ECC (d >= 8, i.e. Golay)
constraint forall(f in 1..num_files)(
  if file_bytes[f] < 1024 then ecc_d[file_ecc[f]] >= 8 else true endif
);

% Medium files (1KB-10KB) must use at least basic ECC (d >= 3)
constraint forall(f in 1..num_files)(
  if file_bytes[f] >= 1024 /\ file_bytes[f] < 10240 then ecc_d[file_ecc[f]] >= 3 else true endif
);

%% ─── Objective: minimize total cost ───

var int: total_cost = if devnet then 0 else
  sum(f in 1..num_files, p in 1..num_platforms)(alloc[f,p] * cost_microusd[p])
endif;

var int: total_carriers = sum(f in 1..num_files)(file_carriers[f]);

solve minimize total_cost + total_carriers;  % cost first, then minimize carrier count

%% ─── Output ───

output [
  "═══ eRDFa Batch Campaign ═══\n",
  "Files: ", show(num_files), "  Devnet: ", show(devnet), "\n",
  "Total carriers: ", show(total_carriers), "\n",
  "Total cost: $", show(total_cost), "µ\n\n"
] ++ [
  "── " ++ file_name[f] ++ " (" ++ show(file_bytes[f]) ++ " B) ──\n" ++
  "  ECC: " ++ ecc_name[fix(file_ecc[f])] ++
  " [" ++ show(ecc_n[fix(file_ecc[f])]) ++ "," ++
          show(ecc_k[fix(file_ecc[f])]) ++ "," ++
          show(ecc_d[fix(file_ecc[f])]) ++ "]" ++
  " → " ++ show(fix(protected[f])) ++ " B protected\n" ++
  concat([
    if fix(alloc[f,p]) > 0 then
      "  " ++ platform_name[p] ++ ": " ++ show(alloc[f,p]) ++ " × " ++
      show(capacity[p]) ++ " B\n"
    else "" endif
    | p in 1..num_platforms
  ]) ++ "\n"
  | f in 1..num_files
];
