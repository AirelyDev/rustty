/* libxml2-sys bindgen surface.
 *
 * The reference has no hand-written libxml2 binding header: ghostty's
 * pkg/libxml2 exists purely to BUILD the archive that pkg/fontconfig links
 * (`fontconfig`'s `fcxml.c` is the only consumer, via `ENABLE_LIBXML2`). So
 * this header mirrors what that consumer needs — the tree/parser public API —
 * kept minimal per the plan's guidance.
 *
 * bindgen parses this against the vendored libxml2 2.11.5 `include/` tree plus
 * the two `override/` config dirs (`xmlversion.h` / `config.h`), exactly the
 * include path the C build uses.
 */
#include <libxml/parser.h>
#include <libxml/tree.h>
