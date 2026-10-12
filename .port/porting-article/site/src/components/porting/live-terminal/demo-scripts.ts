/** DEMO_SCRIPTS are shell scripts seeded into /home/user/demo; each prints terminal sequences with printf. */
export const DEMO_SCRIPTS: Record<string, string> = {
  "/home/user/demo/truecolor.sh": `#!/bin/bash
# A 24-bit gradient: one SGR 48;2 per cell.
for ((i = 0; i < 64; i++)); do
  r=$((i * 4)); g=$((255 - i * 4)); b=128
  printf '\\033[48;2;%d;%d;%dm ' "$r" "$g" "$b"
done
printf '\\033[0m\\n'
`,
  "/home/user/demo/styles.sh": `#!/bin/bash
printf '\\033[1mbold\\033[0m \\033[2mfaint\\033[0m \\033[3mitalic\\033[0m \\033[5mblink\\033[0m \\033[7minverse\\033[0m \\033[9mstrike\\033[0m\\n'
printf '\\033[4:1msingle\\033[0m \\033[4:2mdouble\\033[0m \\033[4:3mcurly\\033[0m \\033[4:4mdotted\\033[0m \\033[4:5mdashed\\033[0m\\n'
printf '\\033[4:3m\\033[58;2;255;80;80mcurly red underline\\033[0m\\n'
`,
  "/home/user/demo/unicode.sh": `#!/bin/bash
printf '漢字 かな 한글 — wide cells stay aligned\\n'
printf 'e\\xcc\\x81 (combining)  \\xf0\\x9f\\x91\\xa8\\xe2\\x80\\x8d\\xf0\\x9f\\x91\\xa9\\xe2\\x80\\x8d\\xf0\\x9f\\x91\\xa7 (ZWJ family)\\n'
`,
  "/home/user/demo/links.sh": `#!/bin/bash
printf '\\033]8;;https://ghostty.org\\033\\\\Ghostty\\033]8;;\\033\\\\ and '
printf '\\033]8;;https://www.rust-lang.org\\033\\\\Rust\\033]8;;\\033\\\\ (OSC 8 hyperlinks)\\n'
`,
  "/home/user/demo/title.sh": `#!/bin/bash
printf '\\033]2;set by the shell\\007'
printf 'window title set via OSC 2\\n'
`,
  "/home/user/demo/progress.sh": `#!/bin/bash
# Cursor movement and line rewriting: 20 steps, no sleep needed.
for ((i = 0; i <= 20; i++)); do
  bar=$(printf '%*s' "$i" '' | tr ' ' '#')
  printf '\\r[%-20s] %3d%%' "$bar" "$((i * 5))"
done
printf '\\n'
`,
};
