#!/usr/bin/env bash
# 0.5.3 allocation/network-race gates, run inside the builder container from /workspace/compiler-v053.
# Usage: v053-alloc-gates.sh kit | cli | cargo | bench
export PATH=/usr/local/cargo/bin:$PATH CARGO_TARGET_DIR=/target/compiler-v053
KIT=/workspace/v053-kit4; CLI=/target/compiler-v053/release/beskid_cli
export BESKID_RUNTIME_PREFIX=$KIT
L=/workspace/v053-alloc
cd /workspace/compiler-v053
case "$1" in
  kit)
    out=$L-kit.log; : > $out
    for p in release debug; do
      $CLI runtime-kit build-native-host --prefix $KIT --profile $p >> $out 2>&1; echo "kit $p EXIT=$?" >> $out
    done
    echo DONE >> $out ;;
  cli)
    out=$L-cli.log; : > $out
    smoke() { # project target label
      local root=$L-root-$3; rm -rf $root
      BESKID_CORELIB_ROOT=$root timeout 3600 $CLI test --plain --project $1 --target $2 --target-timeout 3000 > $L-$3.log 2>&1
      echo "$3 EXIT=$? $(grep '^Result' $L-$3.log)" >> $out
    }
    for t in CoreBytesTests CollectionsArrayTests CoreMathTests HttpCodecTests HttpExchangeTests NetworkTcpTests NetworkUdpTests NetworkDnsTests; do
      smoke corelib/beskid_corelib/tests/corelib_tests $t $t
    done
    for t in NetworkNativeTests GcTests; do smoke runtime/beskid/tests/runtime_semantics $t rt-$t; done
    for p in uri:UriSmokeTests codec:CodecSmokeTests connect:ConnectSmokeTests crypto:CryptoSmokeTests x509:X509SmokeTests tls:TlsSmokeTests http2:Http2SmokeTests websocket:WebSocketSmokeTests websocket:WsLargeTests quic:QuicSmokeTests quic:QuicCreditLoss http3:Http3SmokeTests web:WebSmokeTests; do
      smoke corelib/packages/${p%%:*}/tests ${p##*:} pkg-${p##*:}
    done
    echo DONE >> $out ;;
  cargo)
    out=$L-cargo.log; : > $out
    cargo build --release -p beskid_cli > $L-build.log 2>&1; echo "release build EXIT=$?" >> $out
    cargo clippy --workspace --all-targets -- -D warnings > $L-clippy.log 2>&1; echo "clippy EXIT=$?" >> $out
    for t in optional_extern clif_blocks; do
      cargo test -p beskid_cli --test $t > $L-cli-$t.log 2>&1; echo "cli $t EXIT=$? $(grep 'test result' $L-cli-$t.log)" >> $out
    done
    for c in beskid_abi beskid_queries beskid_isle beskid_codegen beskid_engine beskid_aot; do
      cargo test -p $c > $L-test-$c.log 2>&1; code=$?
      echo "$c EXIT=$code ($(grep -E '^test result' $L-test-$c.log | awk '{p+=$4; f+=$6} END {print p" passed, "f" failed"}'))" >> $out
    done
    echo DONE >> $out ;;
  bench)
    out=$L-bench.log; : > $out
    for p in tls:TlsBench tls:TlsMicroBench http2:Http2Bench quic:QuicBench; do
      t=${p##*:}; root=$L-root-$t; rm -rf $root
      start=$(date +%s)
      BESKID_CORELIB_ROOT=$root timeout 5400 $CLI test --plain --project corelib/packages/${p%%:*}/tests --target $t --target-timeout 5000 --matrix-timeout 5400 > $L-bench-$t.log 2>&1
      echo "$t EXIT=$? wall=$(( $(date +%s) - start ))s $(grep '^Result' $L-bench-$t.log)" >> $out
      grep BENCH $L-bench-$t.log >> $out
    done
    echo DONE >> $out ;;
  verify)
    # Rebuild the real CLI, rerun GcTests and the tagged race tests (green).
    out=$L-verify.log; : > $out
    cargo build --release -p beskid_cli > $L-build3.log 2>&1; echo "release build EXIT=$?" >> $out
    for t in GcTests NetworkNativeTests; do
      rm -rf $L-root-v-$t
      BESKID_CORELIB_ROOT=$L-root-v-$t timeout 3600 $CLI test --plain --project runtime/beskid/tests/runtime_semantics --target $t --target-timeout 3000 > $L-v-$t.log 2>&1
      echo "$t EXIT=$? $(grep '^Result' $L-v-$t.log)" >> $out
    done
    echo DONE >> $out ;;
  mutant)
    # Revert only the NetworkFinish fix, rebuild CLI + a mutant kit, expect the race tests red,
    # then restore the source and the real CLI.
    out=$L-mutant.log; : > $out
    F=runtime/beskid/src/Runtime/Network/Operations.bd; cp $F /tmp/v053-ops.bd
    python3 - <<'PY'
import re
p="runtime/beskid/src/Runtime/Network/Operations.bd"; s=open(p).read()
start=s.index("    if winner == EXTERNAL_TIMEOUT && (raw_word_load"); end=s.index("    }\n", start)+6
s=s[:start]+s[end:]; open(p,"w").write(s)
PY
    grep -c "winner = EXTERNAL_READY" $F >> $out
    cargo build --release -p beskid_cli > $L-build-m.log 2>&1; echo "mutant build EXIT=$?" >> $out
    MK=/workspace/v053-kit4-mutant; rm -rf $MK
    $CLI runtime-kit build-native-host --prefix $MK --profile release >> $L-kit-m.log 2>&1; echo "mutant kit release EXIT=$?" >> $out
    $CLI runtime-kit build-native-host --prefix $MK --profile debug >> $L-kit-m.log 2>&1; echo "mutant kit debug EXIT=$?" >> $out
    rm -rf $L-root-m
    BESKID_RUNTIME_PREFIX=$MK BESKID_CORELIB_ROOT=$L-root-m timeout 3600 $CLI test --plain --project runtime/beskid/tests/runtime_semantics --target NetworkNativeTests --include-tag deadline-race --target-timeout 3000 > $L-m-race.log 2>&1
    echo "mutant race EXIT=$? $(grep '^Result' $L-m-race.log)" >> $out
    cp /tmp/v053-ops.bd $F
    cargo build --release -p beskid_cli > $L-build-r.log 2>&1; echo "restore build EXIT=$?" >> $out
    rm -rf $L-root-r
    BESKID_CORELIB_ROOT=$L-root-r timeout 3600 $CLI test --plain --project runtime/beskid/tests/runtime_semantics --target NetworkNativeTests --include-tag deadline-race --target-timeout 3000 > $L-r-race.log 2>&1
    echo "restored race EXIT=$? $(grep '^Result' $L-r-race.log)" >> $out
    echo DONE >> $out ;;
esac
