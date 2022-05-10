
# You must have the Solana cli installed in the normal place
SDK_ROOT=$(shell echo ~/.local/share/solana/install/active_release/bin/sdk)
#SDK_ROOT=$(shell echo ~/.local/share/solana/install/releases/1.9.13/solana-release/bin/sdk)

C_FLAGS := -fno-builtin -fno-zero-initialized-in-bss -fno-data-sections -fno-inline-functions -std=c99 -I$(SDK_ROOT)/bpf/c/inc -O3

BPF_C_FLAGS := -target bpf -fPIC -march=bpfel+solana

SDK_CLANG := $(SDK_ROOT)/bpf/dependencies/bpf-tools/llvm/bin/clang
SDK_LD := $(SDK_ROOT)/bpf/dependencies/bpf-tools/llvm/bin/ld.lld

define program_id_bytes
$(shell solana-keygen pubkey $1 | tr -d '\n' | util/bs58/target/release/bs58 -d | xxd -p | tr -d '\n' |                \
	(while read -n1 a && read -n1 b; do echo -n "(0x$$a<<4)|0x$$b,"; done))
endef

.PHONY: default
##default: target/program_1.so target/program_2.so target/program_3.so target/program_4.so target/program_5.so \
##         target/program_6.so target/program_7.so target/program_8.so target/program_9.so target/program_10.so \
##         client/target/release/client

default: target/program_1.so client/target/release/client

util/bs58/target/release/bs58: util/bs58/src/main.rs
	cargo build --manifest-path util/bs58/Cargo.toml --release

client/target/release/client: client/src/main.rs
	cargo build --manifest-path client/Cargo.toml --release

target/program_1.po: $(wildcard program/*.c program/*.h) util/bs58/target/release/bs58
	@mkdir -p $(dir $@)
	$(SDK_CLANG) $(C_FLAGS) $(BPF_C_FLAGS)                                                                         \
            -DSELF_PROGRAM_ID_BYTES="$(call program_id_bytes,keys/program_1.json)"                                     \
            -DPROGRAM_PADDING_SIZE=0                                                                                   \
            -o $@ -c program/entrypoint.c

target/program_1.so: target/program_1.po 
	@mkdir -p $(dir $@)
	$(SDK_LD) -z notext -shared --Bdynamic $(SDK_ROOT)/bpf/c/bpf.ld --entry entrypoint -o $@ $^
	strip -s -R .comment $@

target/program_2.po: $(wildcard program/*.c program/*.h)
	@mkdir -p $(dir $@)
	$(SDK_CLANG) $(C_FLAGS) $(BPF_C_FLAGS)                                                                         \
            -DSELF_PROGRAM_ID_BYTES="$(call program_id_bytes,keys/program_2.json)"                                     \
            -DPROGRAM_PADDING_SIZE=1024                                                                                \
            -o $@ -c program/entrypoint.c

target/program_2.so: target/program_2.po
	@mkdir -p $(dir $@)
	$(SDK_LD) -z notext -shared --Bdynamic $(SDK_ROOT)/bpf/c/bpf.ld --entry entrypoint -o $@ $^
	strip -s -R .comment $@

target/program_3.po: $(wildcard program/*.c program/*.h)
	@mkdir -p $(dir $@)
	$(SDK_CLANG) $(C_FLAGS) $(BPF_C_FLAGS)                                                                         \
            -DSELF_PROGRAM_ID_BYTES="$(call program_id_bytes,keys/program_3.json)"                                     \
            -DPROGRAM_PADDING_SIZE=8192                                                                                \
            -o $@ -c program/entrypoint.c

target/program_3.so: target/program_3.po
	@mkdir -p $(dir $@)
	$(SDK_LD) -z notext -shared --Bdynamic $(SDK_ROOT)/bpf/c/bpf.ld --entry entrypoint -o $@ $^
	strip -s -R .comment $@

target/program_4.po: $(wildcard program/*.c program/*.h)
	@mkdir -p $(dir $@)
	$(SDK_CLANG) $(C_FLAGS) $(BPF_C_FLAGS)                                                                         \
            -DSELF_PROGRAM_ID_BYTES="$(call program_id_bytes,keys/program_4.json)"                                     \
            -DPROGRAM_PADDING_SIZE=16384                                                                               \
            -o $@ -c program/entrypoint.c

target/program_4.so: target/program_4.po
	@mkdir -p $(dir $@)
	$(SDK_LD) -z notext -shared --Bdynamic $(SDK_ROOT)/bpf/c/bpf.ld --entry entrypoint -o $@ $^
	strip -s -R .comment $@

target/program_5.po: $(wildcard program/*.c program/*.h)
	@mkdir -p $(dir $@)
	$(SDK_CLANG) $(C_FLAGS) $(BPF_C_FLAGS)                                                                         \
            -DSELF_PROGRAM_ID_BYTES="$(call program_id_bytes,keys/program_5.json)"                                     \
            -DPROGRAM_PADDING_SIZE=32768                                                                               \
            -o $@ -c program/entrypoint.c

target/program_5.so: target/program_5.po
	@mkdir -p $(dir $@)
	$(SDK_LD) -z notext -shared --Bdynamic $(SDK_ROOT)/bpf/c/bpf.ld --entry entrypoint -o $@ $^
	strip -s -R .comment $@

target/program_6.po: $(wildcard program/*.c program/*.h)
	@mkdir -p $(dir $@)
	$(SDK_CLANG) $(C_FLAGS) $(BPF_C_FLAGS)                                                                         \
            -DSELF_PROGRAM_ID_BYTES="$(call program_id_bytes,keys/program_6.json)"                                     \
            -DPROGRAM_PADDING_SIZE=65536                                                                               \
            -o $@ -c program/entrypoint.c

target/program_6.so: target/program_6.po
	@mkdir -p $(dir $@)
	$(SDK_LD) -z notext -shared --Bdynamic $(SDK_ROOT)/bpf/c/bpf.ld --entry entrypoint -o $@ $^
	strip -s -R .comment $@

target/program_7.po: $(wildcard program/*.c program/*.h)
	@mkdir -p $(dir $@)
	$(SDK_CLANG) $(C_FLAGS) $(BPF_C_FLAGS)                                                                         \
            -DSELF_PROGRAM_ID_BYTES="$(call program_id_bytes,keys/program_7.json)"                                     \
            -DPROGRAM_PADDING_SIZE=131072                                                                              \
            -o $@ -c program/entrypoint.c

target/program_7.so: target/program_7.po
	@mkdir -p $(dir $@)
	$(SDK_LD) -z notext -shared --Bdynamic $(SDK_ROOT)/bpf/c/bpf.ld --entry entrypoint -o $@ $^
	strip -s -R .comment $@

target/program_8.po: $(wildcard program/*.c program/*.h)
	@mkdir -p $(dir $@)
	$(SDK_CLANG) $(C_FLAGS) $(BPF_C_FLAGS)                                                                         \
            -DSELF_PROGRAM_ID_BYTES="$(call program_id_bytes,keys/program_8.json)"                                     \
            -DPROGRAM_PADDING_SIZE=262144                                                                              \
            -o $@ -c program/entrypoint.c

target/program_8.so: target/program_8.po
	@mkdir -p $(dir $@)
	$(SDK_LD) -z notext -shared --Bdynamic $(SDK_ROOT)/bpf/c/bpf.ld --entry entrypoint -o $@ $^
	strip -s -R .comment $@

target/program_9.po: $(wildcard program/*.c program/*.h)
	@mkdir -p $(dir $@)
	$(SDK_CLANG) $(C_FLAGS) $(BPF_C_FLAGS)                                                                         \
            -DSELF_PROGRAM_ID_BYTES="$(call program_id_bytes,keys/program_9.json)"                                     \
            -DPROGRAM_PADDING_SIZE=15000                                                                               \
            -o $@ -c program/entrypoint.c

target/program_9.so: target/program_9.po
	@mkdir -p $(dir $@)
	$(SDK_LD) -z notext -shared --Bdynamic $(SDK_ROOT)/bpf/c/bpf.ld --entry entrypoint -o $@ $^
	strip -s -R .comment $@

target/program_10.po: $(wildcard program/*.c program/*.h)
	@mkdir -p $(dir $@)
	$(SDK_CLANG) $(C_FLAGS) $(BPF_C_FLAGS)                                                                         \
            -DSELF_PROGRAM_ID_BYTES="$(call program_id_bytes,keys/program_10.json)"                                    \
            -DPROGRAM_PADDING_SIZE=115000                                                                              \
            -o $@ -c program/entrypoint.c

target/program_10.so: target/program_10.po
	@mkdir -p $(dir $@)
	$(SDK_LD) -z notext -shared --Bdynamic $(SDK_ROOT)/bpf/c/bpf.ld --entry entrypoint -o $@ $^
	strip -s -R .comment $@

.PHONY: clean
clean:
	rm -rf target client/target
