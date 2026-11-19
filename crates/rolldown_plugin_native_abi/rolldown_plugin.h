/* Rolldown experimental native plugin ABI v1. Pin the Rolldown and SDK
 * versions when evaluating it in production; the API may change before
 * stabilization.
 *
 * Inputs are borrowed for one call. Outputs must be allocated with
 * host->buffer_alloc, fully initialized, and returned with data, len, and
 * capacity unchanged; Rolldown owns them after the call returns. Hooks may be
 * called concurrently, so plugin state must be thread-safe. No panic or
 * exception may unwind across this boundary.
 */
#ifndef ROLLDOWN_PLUGIN_H
#define ROLLDOWN_PLUGIN_H

#include <stddef.h>
#include <stdint.h>

#if defined(_WIN32)
#define ROLLDOWN_PLUGIN_EXPORT __declspec(dllexport)
#elif defined(__GNUC__) || defined(__clang__)
#define ROLLDOWN_PLUGIN_EXPORT __attribute__((visibility("default")))
#else
#define ROLLDOWN_PLUGIN_EXPORT
#endif

#ifdef __cplusplus
extern "C" {
#endif

#define ROLLDOWN_ABI_VERSION 1u
#define ROLLDOWN_STATUS_OK 0u
#define ROLLDOWN_STATUS_ERROR 1u
#define ROLLDOWN_STATUS_INCOMPATIBLE 2u

#define ROLLDOWN_HOOK_BUILD_START (1ull << 0)
#define ROLLDOWN_HOOK_RESOLVE_ID (1ull << 1)
#define ROLLDOWN_HOOK_LOAD (1ull << 2)
#define ROLLDOWN_HOOK_TRANSFORM (1ull << 3)
#define ROLLDOWN_HOOK_BUILD_END (1ull << 4)

#define ROLLDOWN_EXTERNAL_NONE 0u
#define ROLLDOWN_EXTERNAL_FALSE 1u
#define ROLLDOWN_EXTERNAL_TRUE 2u
#define ROLLDOWN_EXTERNAL_ABSOLUTE 3u
#define ROLLDOWN_EXTERNAL_RELATIVE 4u

#define ROLLDOWN_SIDE_EFFECTS_NONE 0u
#define ROLLDOWN_SIDE_EFFECTS_TRUE 1u
#define ROLLDOWN_SIDE_EFFECTS_FALSE 2u
#define ROLLDOWN_SIDE_EFFECTS_NO_TREESHAKE 3u

#define ROLLDOWN_IMPORT_KIND_IMPORT 0u
#define ROLLDOWN_IMPORT_KIND_DYNAMIC_IMPORT 1u
#define ROLLDOWN_IMPORT_KIND_REQUIRE 2u
#define ROLLDOWN_IMPORT_KIND_AT_IMPORT 3u
#define ROLLDOWN_IMPORT_KIND_URL_IMPORT 4u
#define ROLLDOWN_IMPORT_KIND_NEW_URL 5u
#define ROLLDOWN_IMPORT_KIND_HOT_ACCEPT 6u

typedef struct RolldownHeader {
  uint32_t abi_version;
  size_t size;
} RolldownHeader;

typedef struct RolldownStringView {
  const uint8_t *data;
  size_t len;
} RolldownStringView;

typedef struct RolldownOptionalStringView {
  RolldownStringView value;
  uint8_t present;
  uint8_t reserved[7];
} RolldownOptionalStringView;

typedef struct RolldownBuffer {
  uint8_t *data;
  size_t len;
  /* Opaque host allocation capacity. Plugins must preserve this value. */
  size_t capacity;
} RolldownBuffer;

typedef struct RolldownOptionalBuffer {
  RolldownBuffer value;
  uint8_t present;
  uint8_t reserved[7];
} RolldownOptionalBuffer;

typedef uint8_t (*RolldownBufferAlloc)(size_t len, RolldownBuffer *output);
typedef uint8_t (*RolldownAddWatchFile)(void *context, RolldownStringView path);

typedef struct RolldownHostApi {
  RolldownHeader header;
  RolldownBufferAlloc buffer_alloc;
  RolldownAddWatchFile add_watch_file;
} RolldownHostApi;

typedef struct RolldownHookContext {
  RolldownHeader header;
  const RolldownHostApi *host;
  void *handle;
  RolldownStringView cwd;
} RolldownHookContext;

typedef struct RolldownInitArgs {
  RolldownHeader header;
  const RolldownHostApi *host;
  RolldownStringView config;
} RolldownInitArgs;

typedef struct RolldownBuildStartArgs {
  RolldownHeader header;
  RolldownHookContext context;
} RolldownBuildStartArgs;

typedef struct RolldownResolveIdArgs {
  RolldownHeader header;
  RolldownHookContext context;
  RolldownStringView specifier;
  RolldownOptionalStringView importer;
  uint8_t kind;
  uint8_t is_entry;
  uint8_t reserved[6];
} RolldownResolveIdArgs;

typedef struct RolldownLoadArgs {
  RolldownHeader header;
  RolldownHookContext context;
  RolldownStringView id;
} RolldownLoadArgs;

typedef struct RolldownTransformArgs {
  RolldownHeader header;
  RolldownHookContext context;
  RolldownStringView id;
  RolldownStringView code;
  RolldownStringView module_type;
} RolldownTransformArgs;

typedef struct RolldownBuildEndArgs {
  RolldownHeader header;
  RolldownHookContext context;
  size_t error_count;
} RolldownBuildEndArgs;

typedef struct RolldownResolveIdOutput {
  RolldownHeader header;
  uint8_t handled;
  uint8_t external;
  uint8_t side_effects;
  uint8_t reserved[5];
  RolldownBuffer id;
} RolldownResolveIdOutput;

typedef struct RolldownLoadOutput {
  RolldownHeader header;
  uint8_t handled;
  uint8_t side_effects;
  uint8_t reserved[6];
  RolldownBuffer code;
  RolldownOptionalBuffer map;
  RolldownOptionalBuffer module_type;
} RolldownLoadOutput;

typedef struct RolldownTransformOutput {
  RolldownHeader header;
  uint8_t handled;
  uint8_t side_effects;
  uint8_t reserved[6];
  RolldownOptionalBuffer code;
  RolldownOptionalBuffer map;
  RolldownOptionalBuffer module_type;
} RolldownTransformOutput;

typedef void (*RolldownDeinit)(void *state);

typedef uint8_t (*RolldownBuildStart)(
  void *state,
  const RolldownBuildStartArgs *args,
  RolldownBuffer *error
);

typedef uint8_t (*RolldownResolveId)(
  void *state,
  const RolldownResolveIdArgs *args,
  RolldownResolveIdOutput *output,
  RolldownBuffer *error
);

typedef uint8_t (*RolldownLoad)(
  void *state,
  const RolldownLoadArgs *args,
  RolldownLoadOutput *output,
  RolldownBuffer *error
);

typedef uint8_t (*RolldownTransform)(
  void *state,
  const RolldownTransformArgs *args,
  RolldownTransformOutput *output,
  RolldownBuffer *error
);

typedef uint8_t (*RolldownBuildEnd)(
  void *state,
  const RolldownBuildEndArgs *args,
  RolldownBuffer *error
);

typedef struct RolldownPluginDescriptor {
  RolldownHeader header;
  RolldownStringView name;
  void *state;
  uint64_t hook_usage;
  RolldownDeinit deinit;
  RolldownBuildStart build_start;
  RolldownResolveId resolve_id;
  RolldownLoad load;
  RolldownTransform transform;
  RolldownBuildEnd build_end;
} RolldownPluginDescriptor;

/* The single required exported symbol. The host initializes plugin->header;
 * its size is the writable descriptor capacity and must be validated before
 * writing the remaining fields. */
ROLLDOWN_PLUGIN_EXPORT uint8_t rolldown_plugin_init(
  const RolldownInitArgs *args,
  RolldownPluginDescriptor *plugin,
  RolldownBuffer *error
);

#ifdef __cplusplus
}
#endif

#endif
