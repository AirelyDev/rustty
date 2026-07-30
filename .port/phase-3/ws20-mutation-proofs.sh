#!/usr/bin/env bash
# WS20 mutation proofs for the new GTK unit tests (cgroup / pre_exec /
# post_fork / flatpak). Each step: back the file up, apply a mutation that
# SHOULD break a named test, run it, print RED/VACUOUS, restore.
#
# The headed ConfigErrorsDialog KAT (tests/gtk_config_errors_dialog.rs) is
# mutation-proved separately, by hand, because it needs a live compositor.
#
# Run from the repo root on the Linux VM (GTK is Linux-only):
#   bash .port/phase-3/ws20-mutation-proofs.sh
set -u

G=crates/rustty-apprt/src/gtk

mutate() { # <file> <sed-expr> <cargo-test-filter> <label>
  local f="$1" expr="$2" filter="$3" label="$4"
  cp "$f" "$f.bak"
  sed -i "$expr" "$f"
  if diff -q "$f" "$f.bak" >/dev/null; then
    echo "NO-OP!!  $label  (sed matched nothing)"
  else
    touch "$f"
    if cargo test -p rustty-apprt --features gtk --lib "$filter" 2>&1 | tail -3 | grep -q "0 failed"; then
      echo "VACUOUS  $label"
    else
      echo "RED (ok) $label"
    fi
  fi
  mv "$f.bak" "$f"
  touch "$f"
}

mutate "$G/cgroup.rs" \
  's|const SCOPE_TAIL: &str = ".scope";|const SCOPE_TAIL: \&str = ".scopeX";|' \
  gtk::cgroup "cgroup::fmt_scope — scope-name tail"

mutate "$G/pre_exec.rs" \
  's|return Some(127);|return Some(126);|g' \
  gtk::pre_exec "pre_exec — the 127 exit codes"

mutate "$G/pre_exec.rs" \
  's|for &b in b"/proc/" {|for \&b in b"/prod/" {|' \
  gtk::pre_exec::tests::current_cgroup "pre_exec — alloc-free /proc/<pid>/cgroup reader"

mutate "$G/post_fork.rs" \
  's|LinuxCgroup::Never => return Ok(()),|LinuxCgroup::Never => return Err(PostForkError),|' \
  gtk::post_fork "post_fork — .never short-circuit"

mutate "$G/post_fork.rs" \
  's|GtkSingleInstance::False => return Ok(()),|GtkSingleInstance::False => return Err(PostForkError),|' \
  gtk::post_fork "post_fork — single-instance=false short-circuit"

mutate "$G/post_fork.rs" \
  's|return Err(PostForkError);|return Ok(());|g' \
  gtk::post_fork "post_fork — the hard-error arms (detect / unknown pid)"

mutate "$G/flatpak.rs" \
  's|    rustty_os::resourcesDir()|    Ok(ResourcesDir::from_paths(Some(String::from("/mutant")), None))|' \
  gtk::flatpak "flatpak — passthrough to internal_os.resourcesDir"

# Restore any stragglers.
find "$G" -name '*.bak' -print -exec sh -c 'mv "$1" "${1%.bak}"' _ {} \;
echo done
