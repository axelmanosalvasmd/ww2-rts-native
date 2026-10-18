/* Direct IF-11 query driver. Link against ww2-ffi as for link_check.c.
 * Usage: motion_check movement_profile '{"type":"tank"}'
 * stdout contains only the returned JSON value; diagnostics go to stderr.
 * The Node harness invokes this bounded argv interface once per query. */
#include <inttypes.h>
#include <stdio.h>
#include <string.h>

#include "ww2_core.h"

#define JSON_LIMIT 65536

typedef int32_t (*query_fn)(const char *, size_t, char *, size_t, size_t *);

static query_fn select_query(const char *name) {
  if (strcmp(name, "movement_profile") == 0) return ww2_movement_profile;
  if (strcmp(name, "is_ground_vehicle") == 0) return ww2_is_ground_vehicle;
  if (strcmp(name, "body_radius") == 0) return ww2_body_radius;
  if (strcmp(name, "flight_at") == 0) return ww2_flight_at;
  if (strcmp(name, "velocity_at") == 0) return ww2_velocity_at;
  return NULL;
}

int main(int argc, char **argv) {
  if (argc != 3) {
    fprintf(stderr, "usage: motion_check QUERY JSON\n");
    return 2;
  }
  query_fn query = select_query(argv[1]);
  if (!query) {
    fprintf(stderr, "unknown motion query\n");
    return 2;
  }
  size_t length = 0;
  while (length <= JSON_LIMIT && argv[2][length]) length++;
  if (!length || length > JSON_LIMIT) {
    fprintf(stderr, "input must contain 1 to %d JSON bytes\n", JSON_LIMIT);
    return 2;
  }
  char out[JSON_LIMIT];
  size_t written = 0;
  int32_t status = query(argv[2], length, out, sizeof out, &written);
  if (status != WW2_OK) {
    fprintf(stderr, "%s returned status %" PRId32 "\n", argv[1], status);
    return 1;
  }
  if (!written || written > sizeof out) {
    fprintf(stderr, "invalid output length %zu\n", written);
    return 1;
  }
  if (fwrite(out, 1, written, stdout) != written || putchar('\n') == EOF ||
      fflush(stdout) == EOF) return 1;
  return 0;
}
