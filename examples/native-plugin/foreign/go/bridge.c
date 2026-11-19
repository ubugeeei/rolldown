#include "bridge.h"

#include <string.h>

extern uint8_t rolldown_go_write_output(uint8_t *data, size_t capacity, size_t *length);

static const char PLUGIN_NAME[] = "example:native-go";
static const char TARGET_MODULE[] = "conformance.foreign.js";

static uint8_t is_conformance_module(const RolldownTransformArgs *args) {
  const size_t target_length = sizeof(TARGET_MODULE) - 1;
  return args != NULL && args->id.data != NULL && args->id.len >= target_length &&
         memcmp(args->id.data + args->id.len - target_length, TARGET_MODULE, target_length) == 0;
}

static uint8_t transform(
  void *state,
  const RolldownTransformArgs *args,
  RolldownTransformOutput *output,
  RolldownBuffer *error
) {
  (void)state;
  (void)error;

  if (args == NULL || output == NULL || args->header.abi_version != ROLLDOWN_ABI_VERSION ||
      args->header.size < sizeof(*args) ||
      args->context.header.abi_version != ROLLDOWN_ABI_VERSION ||
      args->context.header.size < sizeof(args->context) ||
      output->header.abi_version != ROLLDOWN_ABI_VERSION || output->header.size < sizeof(*output)) {
    return ROLLDOWN_STATUS_INCOMPATIBLE;
  }
  if (!is_conformance_module(args)) {
    return ROLLDOWN_STATUS_OK;
  }

  uint8_t source[64];
  size_t length = 0;
  if (rolldown_go_write_output(source, sizeof(source), &length) != ROLLDOWN_STATUS_OK ||
      length > sizeof(source)) {
    return ROLLDOWN_STATUS_ERROR;
  }

  const RolldownHostApi *host = args->context.host;
  if (host == NULL || host->buffer_alloc == NULL ||
      host->header.abi_version != ROLLDOWN_ABI_VERSION || host->header.size < sizeof(*host)) {
    return ROLLDOWN_STATUS_INCOMPATIBLE;
  }
  if (host->buffer_alloc(length, &output->code.value) != ROLLDOWN_STATUS_OK) {
    return ROLLDOWN_STATUS_ERROR;
  }
  if (output->code.value.data == NULL || output->code.value.len != length ||
      output->code.value.capacity < length) {
    return ROLLDOWN_STATUS_ERROR;
  }

  memcpy(output->code.value.data, source, length);
  output->handled = 1;
  output->code.present = 1;
  return ROLLDOWN_STATUS_OK;
}

ROLLDOWN_PLUGIN_EXPORT uint8_t rolldown_plugin_init(
  const RolldownInitArgs *args,
  RolldownPluginDescriptor *plugin,
  RolldownBuffer *error
) {
  (void)error;

  if (args == NULL || plugin == NULL || args->header.abi_version != ROLLDOWN_ABI_VERSION ||
      args->header.size < sizeof(*args) || args->host == NULL ||
      args->host->header.abi_version != ROLLDOWN_ABI_VERSION ||
      args->host->header.size < sizeof(*args->host) ||
      plugin->header.abi_version != ROLLDOWN_ABI_VERSION || plugin->header.size < sizeof(*plugin)) {
    return ROLLDOWN_STATUS_INCOMPATIBLE;
  }

  memset(plugin, 0, sizeof(*plugin));
  plugin->header.abi_version = ROLLDOWN_ABI_VERSION;
  plugin->header.size = sizeof(*plugin);
  plugin->name.data = (const uint8_t *)PLUGIN_NAME;
  plugin->name.len = sizeof(PLUGIN_NAME) - 1;
  plugin->hook_usage = ROLLDOWN_HOOK_TRANSFORM;
  plugin->transform = transform;
  return ROLLDOWN_STATUS_OK;
}
