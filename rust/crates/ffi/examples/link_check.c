/* Links the static library through include/ww2_core.h, the way the Unreal plugin will, and checks it
 * against the JavaScript. From rust/, on Linux:
 *
 *   cargo build -p ww2-ffi --release --target x86_64-unknown-linux-gnu
 *   cc -std=c11 -Wall -Wextra -I crates/ffi/include crates/ffi/examples/link_check.c \
 *     target/x86_64-unknown-linux-gnu/release/libww2_ffi.a -lgcc_s -lutil -lrt -lpthread -lm -ldl -lc \
 *     -o link_check
 *   ./link_check crates/core/tests/fixtures/formation.txt [server-messages.jsonl]
 *
 * The libraries after the archive are what `cargo rustc -p ww2-ffi --release -- --print
 * native-static-libs` lists for that target. The program prints the interface version, the rules
 * hash and two slot sizes, checks that bad arguments come back as error codes, then replays every
 * written-out case of the formation fixture (from harness/formation-fixture.mjs) through
 * ww2_formation_spots and compares each spot bit for bit with the pinned JavaScript's. Cases of six
 * units or fewer are printed in full. It exits 0 when everything matches. */
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "ww2_core.h"

#define MAX_ITEMS 1024

_Static_assert(sizeof(ww2_view_unit) == 152, "ABI 2 unit row size");
_Static_assert(offsetof(ww2_view_unit, kind) == 8, "ABI 2 type name offset");
_Static_assert(offsetof(ww2_view_unit, target) == 136, "ABI 2 target offset");

static double from_bits(uint64_t bits) {
  double x;
  memcpy(&x, &bits, sizeof x);
  return x;
}

/* The bits the fixture records: JavaScript has one NaN. */
static uint64_t bits_of(double x) {
  uint64_t bits;
  if (x != x) return UINT64_C(0x7ff8000000000000);
  memcpy(&bits, &x, sizeof bits);
  return bits;
}

static int32_t shape_code(const char *name) {
  if (strcmp(name, "line") == 0) return WW2_SHAPE_LINE;
  if (strcmp(name, "block") == 0) return WW2_SHAPE_BLOCK;
  if (strcmp(name, "column") == 0) return WW2_SHAPE_COLUMN;
  if (strcmp(name, "wedge") == 0) return WW2_SHAPE_WEDGE;
  return -1;
}

static void print_last_error(void) {
  char buf[256];
  size_t len = 0;
  if (ww2_last_error(buf, sizeof buf, &len) == WW2_OK) printf("%.*s", (int)len, buf);
}

/* Prints a call's code and message; returns 1 when the code is not the expected one. */
static int expect(const char *what, int32_t code, int32_t want) {
  printf("%s: %d (", what, code);
  print_last_error();
  printf(")%s\n", code == want ? "" : ", WRONG CODE");
  return code != want;
}

/* Bad arguments return codes and leave a message; returns how many did not. */
static int check_errors(void) {
  ww2_formation_item item = {.id = 1, .back = 0, .x = 0, .z = 0, .size = 3};
  ww2_formation_spot spot;
  size_t len = 0;
  int failures = 0;
  failures += expect("unknown shape 9", ww2_formation_spots(&item, 1, 0, 0, 0, 0, 9, 1, &spot, 1, &len),
                     WW2_ERR_BAD_ARGUMENT);
  failures += expect("no room for the spot",
                     ww2_formation_spots(&item, 1, 0, 0, 0, 0, WW2_SHAPE_LINE, 1, &spot, 0, &len), WW2_ERR_CAPACITY);
  failures += expect("null items", ww2_formation_spots(NULL, 1, 0, 0, 0, 0, WW2_SHAPE_LINE, 1, &spot, 1, &len),
                     WW2_ERR_NULL_POINTER);
  failures += expect("null out_len", ww2_formation_spots(&item, 1, 0, 0, 0, 0, WW2_SHAPE_LINE, 1, &spot, 1, NULL),
                     WW2_ERR_NULL_POINTER);
  return failures;
}

static ww2_formation_item items[MAX_ITEMS];
static ww2_formation_spot expected[MAX_ITEMS], got[MAX_ITEMS];

struct written_case {
  char label[64], shape[16];
  double spread, reach, face, at_x, at_z;
  size_t items, spots, items_read, spots_read;
};

/* Runs one written-out case through the library; returns 1 when it differs from the JavaScript. */
static int run_case(const struct written_case *c) {
  size_t len = 0;
  int32_t code = ww2_formation_spots(items, c->items, c->at_x, c->at_z, c->face, c->reach, shape_code(c->shape),
                                     c->spread, got, MAX_ITEMS, &len);
  int differs = code != WW2_OK || len != c->spots || c->items_read != c->items || c->spots_read != c->spots;
  for (size_t i = 0; !differs && i < len; i++) {
    differs = got[i].id != expected[i].id || bits_of(got[i].x) != bits_of(expected[i].x) ||
              bits_of(got[i].z) != bits_of(expected[i].z);
  }
  if (differs || c->items <= 6) {
    printf("case %s: %s, spread %g, reach %g, face %.17g, %zu units -> %zu spots, %s\n", c->label, c->shape,
           c->spread, c->reach, c->face, c->items, len, differs ? "DIFFERS from the JavaScript" : "bit-equal");
    for (size_t i = 0; i < len; i++) {
      printf("  %10" PRIu32 " %.17g %.17g\n", got[i].id, got[i].x, got[i].z);
    }
  }
  if (code != WW2_OK) {
    printf("  error %d: ", code);
    print_last_error();
    printf("\n");
  }
  return differs;
}

/* Replays raw server JSON messages, one per line, without a platform-specific getline. */
static int check_view(const char *path) {
  FILE *f = fopen(path, "rb");
  if (!f) { printf("cannot open %s\n", path); return 1; }
  ww2_view *view = NULL;
  if (ww2_view_new(&view) != WW2_OK) { fclose(f); return 1; }
  char *line = NULL;
  size_t capacity = 0, length = 0, messages = 0;
  int failures = 0, ch;
  while (!failures) {
    ch = fgetc(f);
    if (ch == EOF || ch == '\n') {
      if (length > 0) {
        int32_t code = ww2_view_apply(view, line, length);
        if (code != WW2_OK) {
          printf("view message %zu: %d (", messages + 1, code);
          print_last_error();
          printf(")\n");
          failures = 1;
        }
        messages++;
        length = 0;
      }
      if (ch == EOF) break;
      continue;
    }
    if (length == capacity) {
      size_t next = capacity ? capacity * 2 : 4096;
      if (next > 16 * 1024 * 1024) {
        printf("view message exceeds 16 MiB\n"); failures = 1; break;
      }
      char *grown = realloc(line, next);
      if (!grown) { failures = 1; break; }
      line = grown;
      capacity = next;
    }
    line[length++] = (char)ch;
  }
  if (ferror(f)) failures = 1;
  free(line);
  fclose(f);
  size_t count = 0, written = 0;
  uint8_t resync = 0;
  if (ww2_view_unit_count(view, &count) != WW2_OK ||
      ww2_view_needs_resync(view, &resync) != WW2_OK) failures = 1;
  ww2_view_unit *units = count ? calloc(count, sizeof *units) : NULL;
  if (count && !units) failures = 1;
  if (!failures && ww2_view_units(view, units, count, &written) != WW2_OK) {
    printf("view rows: "); print_last_error(); printf("\n"); failures = 1;
  }
  printf("%zu view messages: %zu units, needs_resync=%u\n", messages, count, (unsigned)resync);
  for (size_t i = 0; !failures && i < written && i < 5; i++) {
    printf("  unit %" PRIu32 " %s owner=%" PRId32 " x=%.17g z=%.17g hp=%.17g flags=%" PRIu32 "\n",
           units[i].id, units[i].kind, units[i].owner, units[i].x, units[i].z, units[i].hp, units[i].flags);
  }
  free(units);
  if (ww2_view_free(view) != WW2_OK) failures = 1;
  return failures;
}

int main(int argc, char **argv) {
  printf("ww2_abi_version() = %" PRIu32 " (header WW2_ABI_VERSION %d)\n", ww2_abi_version(), WW2_ABI_VERSION);
  printf("ww2_rules_hash() = %016" PRIx64 "\n", ww2_rules_hash());
  double infantry = 0, tank = 0;
  ww2_formation_slot_size(1, 0.5, &infantry);
  ww2_formation_slot_size(0, 2.2, &tank);
  printf("slot sizes: infantry radius 0.5 -> %.17g, vehicle radius 2.2 -> %.17g\n", infantry, tank);
  int failures = (ww2_abi_version() != WW2_ABI_VERSION) + check_errors();

  if (argc > 1) {
    FILE *f = fopen(argv[1], "r");
    if (!f) {
      printf("cannot open %s\n", argv[1]);
      return 1;
    }
    char line[1024];
    struct written_case c;
    int open = 0;
    size_t cases = 0, spots = 0, differing = 0;
    for (;;) {
      int more = fgets(line, sizeof line, f) != NULL;
      int is_item = more && strncmp(line, "item ", 5) == 0, is_spot = more && strncmp(line, "spot ", 5) == 0;
      if (open && !is_item && !is_spot) {
        differing += run_case(&c);
        cases++;
        spots += c.spots;
        open = 0;
      }
      if (!more) break;
      uint64_t a, b, d, e, g;
      if (strncmp(line, "case ", 5) == 0) {
        memset(&c, 0, sizeof c);
        if (sscanf(line, "case %63s %15s %" SCNx64 " %" SCNx64 " %" SCNx64 " %" SCNx64 " %" SCNx64 " %zu %zu",
                   c.label, c.shape, &a, &b, &d, &e, &g, &c.items, &c.spots) != 9 ||
            c.items > MAX_ITEMS) {
          printf("bad case line: %s", line);
          return 1;
        }
        c.spread = from_bits(a), c.reach = from_bits(b), c.face = from_bits(d);
        c.at_x = from_bits(e), c.at_z = from_bits(g);
        open = 1;
      } else if (is_item && open && c.items_read < MAX_ITEMS) {
        ww2_formation_item *u = &items[c.items_read++];
        unsigned back = 0;
        sscanf(line, "item %" SCNu32 " %" SCNx64 " %" SCNx64 " %" SCNx64 " %u", &u->id, &a, &b, &d, &back);
        u->x = from_bits(a), u->z = from_bits(b), u->size = from_bits(d), u->back = (uint8_t)back;
      } else if (is_spot && open && c.spots_read < MAX_ITEMS) {
        ww2_formation_spot *s = &expected[c.spots_read++];
        sscanf(line, "spot %" SCNu32 " %" SCNx64 " %" SCNx64, &s->id, &a, &b);
        s->x = from_bits(a), s->z = from_bits(b);
      }
    }
    fclose(f);
    printf("%zu written cases, %zu spots: %zu differ from the JavaScript\n", cases, spots, differing);
    failures += differing > 0 || cases == 0;
  }
  if (argc > 2) failures += check_view(argv[2]);
  printf("%s\n", failures ? "FAILED" : "all checks passed");
  return failures ? 1 : 0;
}
