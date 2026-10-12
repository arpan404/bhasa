/* Vulkan 1.0 compute adapter. Source is trusted SPIR-V, never CPU code. */
#define VK_NO_PROTOTYPES
#include "vendor/vulkan_core.h"
#include "vulkan.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef _WIN32
#include <windows.h>
static void *open_library(const char *name) { return (void *)LoadLibraryA(name); }
static void *find_symbol(void *lib, const char *name) { return (void *)(uintptr_t)GetProcAddress((HMODULE)lib, name); }
static void close_library(void *lib) { if (lib) FreeLibrary((HMODULE)lib); }
#else
#include <dlfcn.h>
static void *open_library(const char *name) { return dlopen(name, RTLD_NOW | RTLD_LOCAL); }
static void *find_symbol(void *lib, const char *name) { return dlsym(lib, name); }
static void close_library(void *lib) { if (lib) dlclose(lib); }
#endif

#define VULKAN_FUNCTIONS(X) \
    X(CreateInstance) X(DestroyInstance) X(EnumerateInstanceExtensionProperties) \
    X(EnumeratePhysicalDevices) X(GetPhysicalDeviceProperties) X(GetPhysicalDeviceQueueFamilyProperties) \
    X(GetPhysicalDeviceMemoryProperties) X(EnumerateDeviceExtensionProperties) \
    X(CreateDevice) X(DestroyDevice) X(GetDeviceQueue) X(DeviceWaitIdle) \
    X(CreateBuffer) X(DestroyBuffer) X(GetBufferMemoryRequirements) X(AllocateMemory) X(FreeMemory) \
    X(BindBufferMemory) X(MapMemory) X(UnmapMemory) X(CreateShaderModule) X(DestroyShaderModule) \
    X(CreateDescriptorSetLayout) X(DestroyDescriptorSetLayout) X(CreatePipelineLayout) X(DestroyPipelineLayout) \
    X(CreateComputePipelines) X(DestroyPipeline) X(CreateDescriptorPool) X(DestroyDescriptorPool) \
    X(AllocateDescriptorSets) X(UpdateDescriptorSets) X(CreateCommandPool) X(DestroyCommandPool) \
    X(AllocateCommandBuffers) X(BeginCommandBuffer) X(EndCommandBuffer) X(CmdBindPipeline) \
    X(CmdBindDescriptorSets) X(CmdPushConstants) X(CmdDispatch) X(CmdPipelineBarrier) X(QueueSubmit) X(QueueWaitIdle)

typedef struct {
    void *library;
#define FIELD(name) PFN_vk##name name;
    VULKAN_FUNCTIONS(FIELD)
#undef FIELD
    VkInstance instance;
    VkPhysicalDevice physical;
    VkPhysicalDeviceProperties properties;
    uint32_t family;
    int properties2_enabled;
} Vulkan;

static void shutdown(Vulkan *api) {
    if (api->instance) api->DestroyInstance(api->instance, NULL);
    close_library(api->library);
    memset(api, 0, sizeof(*api));
}

static int load(Vulkan *api) {
    memset(api, 0, sizeof(*api));
    const char *override = getenv("BHASA_VULKAN_LIBRARY");
    if (override && *override) {
        api->library = open_library(override);
    } else {
        static const char *const names[] = {
#ifdef _WIN32
            "vulkan-1.dll",
#elif defined(__APPLE__)
            "libvulkan.1.dylib", "libvulkan.dylib", "libMoltenVK.dylib",
            "/opt/homebrew/lib/libvulkan.1.dylib", "/opt/homebrew/lib/libMoltenVK.dylib",
#else
            "libvulkan.so.1", "libvulkan.so",
#endif
            NULL};
        for (size_t i = 0; !api->library && names[i]; ++i) api->library = open_library(names[i]);
    }
    if (!api->library) return 0;
#define LOAD(name) do { \
    void *symbol = find_symbol(api->library, "vk" #name); \
    if (!symbol || sizeof(api->name) != sizeof(symbol)) goto failed; \
    memcpy(&api->name, &symbol, sizeof(symbol)); \
} while (0);
    VULKAN_FUNCTIONS(LOAD)
#undef LOAD
    return 1;
failed:
    shutdown(api);
    return 0;
}

static int instance_extension(Vulkan *api, const char *name) {
    uint32_t count = 0;
    if (api->EnumerateInstanceExtensionProperties(NULL, &count, NULL) != VK_SUCCESS || count > 65536) return 0;
    VkExtensionProperties *extensions = calloc(count ? count : 1, sizeof(*extensions));
    if (!extensions) return 0;
    int found = 0;
    if (api->EnumerateInstanceExtensionProperties(NULL, &count, extensions) == VK_SUCCESS) {
        for (uint32_t i = 0; i < count; ++i) if (!strcmp(extensions[i].extensionName, name)) found = 1;
    }
    free(extensions);
    return found;
}

static int initialize(Vulkan *api) {
    if (!load(api)) return 0;
    const char *portability = "VK_KHR_portability_enumeration";
    int enable_portability = instance_extension(api, portability);
    const char *properties2 = "VK_KHR_get_physical_device_properties2";
    api->properties2_enabled = instance_extension(api, properties2);
    const char *extensions[2];
    uint32_t extension_count = 0;
    if (enable_portability) extensions[extension_count++] = portability;
    /* Vulkan 1.0 needs this instance extension before enabling portability_subset. */
    if (api->properties2_enabled) extensions[extension_count++] = properties2;
    VkApplicationInfo application = {.sType = VK_STRUCTURE_TYPE_APPLICATION_INFO,
        .pApplicationName = "Bhasa", .apiVersion = VK_API_VERSION_1_0};
    VkInstanceCreateInfo create = {.sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
        .pApplicationInfo = &application,
        /* VK_INSTANCE_CREATE_ENUMERATE_PORTABILITY_BIT_KHR, added after this header. */
        .flags = enable_portability ? 0x00000001u : 0u,
        .enabledExtensionCount = extension_count,
        .ppEnabledExtensionNames = extension_count ? extensions : NULL};
    if (api->CreateInstance(&create, NULL, &api->instance) != VK_SUCCESS) goto failed;
    uint32_t count = 0;
    if (api->EnumeratePhysicalDevices(api->instance, &count, NULL) != VK_SUCCESS || !count || count > 65536) goto failed;
    VkPhysicalDevice *devices = calloc(count, sizeof(*devices));
    if (!devices) goto failed;
    if (api->EnumeratePhysicalDevices(api->instance, &count, devices) != VK_SUCCESS) { free(devices); goto failed; }
    for (uint32_t i = 0; i < count && !api->physical; ++i) {
        VkPhysicalDeviceProperties properties;
        api->GetPhysicalDeviceProperties(devices[i], &properties);
        /* Software Vulkan implementations must not count as GPU execution. */
        if (properties.deviceType != VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU &&
            properties.deviceType != VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU &&
            properties.deviceType != VK_PHYSICAL_DEVICE_TYPE_VIRTUAL_GPU) continue;
        uint32_t families = 0;
        api->GetPhysicalDeviceQueueFamilyProperties(devices[i], &families, NULL);
        if (!families || families > 65536) continue;
        VkQueueFamilyProperties *queues = calloc(families, sizeof(*queues));
        if (!queues) continue;
        api->GetPhysicalDeviceQueueFamilyProperties(devices[i], &families, queues);
        for (uint32_t family = 0; family < families; ++family) {
            if (queues[family].queueCount && (queues[family].queueFlags & VK_QUEUE_COMPUTE_BIT)) {
                api->physical = devices[i];
                api->properties = properties;
                api->family = family;
                break;
            }
        }
        free(queues);
    }
    free(devices);
    if (api->physical) return 1;
failed:
    shutdown(api);
    return 0;
}

static int valid(const bhasa_buffer *buffer) {
    return buffer && buffer->len >= 0 && buffer->cap >= buffer->len &&
        (!buffer->len || buffer->data) && (uint64_t)buffer->len < SIZE_MAX;
}

/* Validate the supported envelope before handing trusted shader code to a driver. */
static int validate_spirv(const uint32_t *words, size_t count, const char *entry) {
    if (count < 5 || words[0] != 0x07230203 || words[1] != 0x00010000 || !words[3] || words[4]) return BHASA_GPU_COMPILE_ERROR;
    uint32_t entry_id = 0;
    for (size_t i = 5; i < count;) {
        uint32_t width = words[i] >> 16, opcode = words[i] & 65535;
        if (!width || width > count - i) return BHASA_GPU_COMPILE_ERROR;
        /* No optional shader capabilities are enabled on the logical device. */
        if (opcode == 17 && (width != 2 || words[i + 1] != 1)) return BHASA_GPU_COMPILE_ERROR;
        if (opcode == 15 && width >= 4 && words[i + 1] == 5) {
            const char *name = (const char *)(words + i + 3);
            size_t bytes = (width - 3) * sizeof(uint32_t);
            if (!memchr(name, 0, bytes)) return BHASA_GPU_COMPILE_ERROR;
            if (!strcmp(name, entry)) entry_id = words[i + 2];
        }
        i += width;
    }
    if (!entry_id) return BHASA_GPU_ENTRY_NOT_FOUND;
    for (size_t i = 5; i < count; i += words[i] >> 16) {
        uint32_t width = words[i] >> 16, opcode = words[i] & 65535;
        if (opcode == 16 && width >= 3 && words[i + 1] == entry_id && words[i + 2] == 17) {
            return width == 6 && words[i + 3] == 1 && words[i + 4] == 1 && words[i + 5] == 1
                ? BHASA_GPU_OK : BHASA_GPU_LIMIT;
        }
    }
    return BHASA_GPU_LIMIT;
}

int64_t bhasa_rt_gpu_available(void) {
    Vulkan api;
    if (!initialize(&api)) return 0;
    shutdown(&api);
    return 1;
}

int64_t bhasa_rt_gpu_dispatch(const bhasa_buffer *source, const bhasa_buffer *entry, bhasa_buffer *data) {
    if (!valid(source) || !valid(entry) || !valid(data) || source->len < 20 || source->len % 4 ||
        !entry->len || memchr(entry->data, 0, (size_t)entry->len)) return BHASA_GPU_INVALID_ARGUMENT;
    if ((uint64_t)data->len > UINT32_MAX || (uint64_t)data->len > SIZE_MAX / sizeof(uint32_t)) return BHASA_GPU_LIMIT;
    uint32_t *words = malloc((size_t)source->len);
    char *name = malloc((size_t)entry->len + 1);
    if (!words || !name) { free(words); free(name); return BHASA_GPU_ALLOCATION_ERROR; }
    memcpy(words, source->data, (size_t)source->len);
    memcpy(name, entry->data, (size_t)entry->len);
    name[entry->len] = 0;
    int64_t status = validate_spirv(words, (size_t)source->len / 4, name);
    if (status) { free(words); free(name); return status; }
    Vulkan api;
    if (!initialize(&api)) {
        free(words); free(name);
        fprintf(stderr, "bhasa GPU: Vulkan loader or hardware compute device is unavailable\n");
        return BHASA_GPU_UNAVAILABLE;
    }
    VkDevice device = VK_NULL_HANDLE;
    VkBuffer buffer = VK_NULL_HANDLE;
    VkDeviceMemory memory = VK_NULL_HANDLE;
    VkShaderModule shader = VK_NULL_HANDLE;
    VkDescriptorSetLayout descriptor_layout = VK_NULL_HANDLE;
    VkPipelineLayout pipeline_layout = VK_NULL_HANDLE;
    VkPipeline pipeline = VK_NULL_HANDLE;
    VkDescriptorPool descriptor_pool = VK_NULL_HANDLE;
    VkCommandPool command_pool = VK_NULL_HANDLE;
    void *mapped = NULL;
    uint32_t *readback = NULL;
    VkDeviceSize bytes = (VkDeviceSize)data->len * sizeof(uint32_t);
    if (bytes > api.properties.limits.maxStorageBufferRange ||
        (uint64_t)data->len > api.properties.limits.maxComputeWorkGroupCount[0]) {
        status = BHASA_GPU_LIMIT; goto cleanup;
    }
    float priority = 1.0f;
    VkDeviceQueueCreateInfo queue_info = {.sType = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
        .queueFamilyIndex = api.family, .queueCount = 1, .pQueuePriorities = &priority};
    uint32_t extension_count = 0;
    int portability_subset = 0;
    status = BHASA_GPU_PIPELINE_ERROR;
    if (api.EnumerateDeviceExtensionProperties(api.physical, NULL, &extension_count, NULL) != VK_SUCCESS || extension_count > 65536) goto cleanup;
    VkExtensionProperties *extensions = calloc(extension_count ? extension_count : 1, sizeof(*extensions));
    if (!extensions) { status = BHASA_GPU_ALLOCATION_ERROR; goto cleanup; }
    VkResult extension_result = api.EnumerateDeviceExtensionProperties(api.physical, NULL, &extension_count, extensions);
    for (uint32_t i = 0; extension_result == VK_SUCCESS && i < extension_count; ++i)
        if (!strcmp(extensions[i].extensionName, "VK_KHR_portability_subset")) portability_subset = 1;
    free(extensions);
    if (extension_result != VK_SUCCESS) goto cleanup;
    if (portability_subset && !api.properties2_enabled) {
        fprintf(stderr, "bhasa GPU: Vulkan portability subset requires VK_KHR_get_physical_device_properties2\n");
        goto cleanup;
    }
    const char *portability_name = "VK_KHR_portability_subset";
    VkDeviceCreateInfo device_info = {.sType = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,
        .queueCreateInfoCount = 1, .pQueueCreateInfos = &queue_info,
        .enabledExtensionCount = portability_subset ? 1u : 0u,
        .ppEnabledExtensionNames = portability_subset ? &portability_name : NULL};
    if (api.CreateDevice(api.physical, &device_info, NULL, &device) != VK_SUCCESS) goto cleanup;
    VkQueue queue;
    api.GetDeviceQueue(device, api.family, 0, &queue);
    VkShaderModuleCreateInfo shader_info = {.sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
        .codeSize = (size_t)source->len, .pCode = words};
    status = BHASA_GPU_COMPILE_ERROR;
    if (api.CreateShaderModule(device, &shader_info, NULL, &shader) != VK_SUCCESS) goto cleanup;
    VkDescriptorSetLayoutBinding binding = {.binding = 0, .descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,
        .descriptorCount = 1, .stageFlags = VK_SHADER_STAGE_COMPUTE_BIT};
    VkDescriptorSetLayoutCreateInfo layout_info = {.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
        .bindingCount = 1, .pBindings = &binding};
    status = BHASA_GPU_PIPELINE_ERROR;
    if (api.CreateDescriptorSetLayout(device, &layout_info, NULL, &descriptor_layout) != VK_SUCCESS) goto cleanup;
    VkPushConstantRange push = {.stageFlags = VK_SHADER_STAGE_COMPUTE_BIT, .offset = 0, .size = sizeof(uint32_t)};
    VkPipelineLayoutCreateInfo pipeline_layout_info = {.sType = VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
        .setLayoutCount = 1, .pSetLayouts = &descriptor_layout, .pushConstantRangeCount = 1, .pPushConstantRanges = &push};
    if (api.CreatePipelineLayout(device, &pipeline_layout_info, NULL, &pipeline_layout) != VK_SUCCESS) goto cleanup;
    VkComputePipelineCreateInfo pipeline_info = {.sType = VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO,
        .stage = {.sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
            .stage = VK_SHADER_STAGE_COMPUTE_BIT, .module = shader, .pName = name},
        .layout = pipeline_layout};
    if (api.CreateComputePipelines(device, VK_NULL_HANDLE, 1, &pipeline_info, NULL, &pipeline) != VK_SUCCESS) goto cleanup;
    status = BHASA_GPU_OK;
    if (!data->len) goto cleanup;
    status = BHASA_GPU_ALLOCATION_ERROR;
    VkBufferCreateInfo buffer_info = {.sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
        .size = bytes, .usage = VK_BUFFER_USAGE_STORAGE_BUFFER_BIT, .sharingMode = VK_SHARING_MODE_EXCLUSIVE};
    if (api.CreateBuffer(device, &buffer_info, NULL, &buffer) != VK_SUCCESS) goto cleanup;
    VkMemoryRequirements requirements;
    VkPhysicalDeviceMemoryProperties memory_properties;
    api.GetBufferMemoryRequirements(device, buffer, &requirements);
    api.GetPhysicalDeviceMemoryProperties(api.physical, &memory_properties);
    uint32_t memory_index = UINT32_MAX;
    for (uint32_t i = 0; i < memory_properties.memoryTypeCount && i < 32; ++i) {
        VkMemoryPropertyFlags flags = memory_properties.memoryTypes[i].propertyFlags;
        if ((requirements.memoryTypeBits & (1u << i)) &&
            (flags & (VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT)) ==
            (VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT)) {
            memory_index = i; break;
        }
    }
    if (memory_index == UINT32_MAX || requirements.size < bytes) { status = BHASA_GPU_LIMIT; goto cleanup; }
    VkMemoryAllocateInfo allocation = {.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        .allocationSize = requirements.size, .memoryTypeIndex = memory_index};
    if (api.AllocateMemory(device, &allocation, NULL, &memory) != VK_SUCCESS ||
        api.BindBufferMemory(device, buffer, memory, 0) != VK_SUCCESS ||
        api.MapMemory(device, memory, 0, bytes, 0, &mapped) != VK_SUCCESS) goto cleanup;
    readback = malloc((size_t)bytes);
    if (!readback) goto cleanup;
    for (int64_t i = 0; i < data->len; ++i) ((uint32_t *)mapped)[i] = data->data[i];
    VkDescriptorPoolSize pool_size = {.type = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, .descriptorCount = 1};
    VkDescriptorPoolCreateInfo pool_info = {.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO,
        .maxSets = 1, .poolSizeCount = 1, .pPoolSizes = &pool_size};
    if (api.CreateDescriptorPool(device, &pool_info, NULL, &descriptor_pool) != VK_SUCCESS) goto cleanup;
    VkDescriptorSetAllocateInfo descriptor_info = {.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,
        .descriptorPool = descriptor_pool, .descriptorSetCount = 1, .pSetLayouts = &descriptor_layout};
    VkDescriptorSet descriptor;
    if (api.AllocateDescriptorSets(device, &descriptor_info, &descriptor) != VK_SUCCESS) goto cleanup;
    VkDescriptorBufferInfo buffer_descriptor = {.buffer = buffer, .offset = 0, .range = bytes};
    VkWriteDescriptorSet write = {.sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
        .dstSet = descriptor, .dstBinding = 0, .descriptorCount = 1,
        .descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, .pBufferInfo = &buffer_descriptor};
    api.UpdateDescriptorSets(device, 1, &write, 0, NULL);
    VkCommandPoolCreateInfo command_pool_info = {.sType = VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,
        .queueFamilyIndex = api.family, .flags = VK_COMMAND_POOL_CREATE_TRANSIENT_BIT};
    if (api.CreateCommandPool(device, &command_pool_info, NULL, &command_pool) != VK_SUCCESS) goto cleanup;
    VkCommandBufferAllocateInfo command_info = {.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
        .commandPool = command_pool, .level = VK_COMMAND_BUFFER_LEVEL_PRIMARY, .commandBufferCount = 1};
    VkCommandBuffer command;
    if (api.AllocateCommandBuffers(device, &command_info, &command) != VK_SUCCESS) goto cleanup;
    VkCommandBufferBeginInfo begin = {.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
        .flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT};
    status = BHASA_GPU_EXECUTION_ERROR;
    if (api.BeginCommandBuffer(command, &begin) != VK_SUCCESS) goto cleanup;
    api.CmdBindPipeline(command, VK_PIPELINE_BIND_POINT_COMPUTE, pipeline);
    api.CmdBindDescriptorSets(command, VK_PIPELINE_BIND_POINT_COMPUTE, pipeline_layout, 0, 1, &descriptor, 0, NULL);
    uint32_t count = (uint32_t)data->len;
    api.CmdPushConstants(command, pipeline_layout, VK_SHADER_STAGE_COMPUTE_BIT, 0, sizeof(count), &count);
    api.CmdDispatch(command, count, 1, 1);
    VkMemoryBarrier barrier = {.sType = VK_STRUCTURE_TYPE_MEMORY_BARRIER,
        .srcAccessMask = VK_ACCESS_SHADER_WRITE_BIT, .dstAccessMask = VK_ACCESS_HOST_READ_BIT};
    api.CmdPipelineBarrier(command, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT, VK_PIPELINE_STAGE_HOST_BIT,
        0, 1, &barrier, 0, NULL, 0, NULL);
    if (api.EndCommandBuffer(command) != VK_SUCCESS) goto cleanup;
    VkSubmitInfo submit = {.sType = VK_STRUCTURE_TYPE_SUBMIT_INFO, .commandBufferCount = 1, .pCommandBuffers = &command};
    if (api.QueueSubmit(queue, 1, &submit, VK_NULL_HANDLE) != VK_SUCCESS || api.QueueWaitIdle(queue) != VK_SUCCESS) goto cleanup;
    memcpy(readback, mapped, (size_t)bytes);
    /* Validate all values before publishing any host bytes. */
    for (int64_t i = 0; i < data->len; ++i) if (readback[i] > 255) goto cleanup;
    for (int64_t i = 0; i < data->len; ++i) data->data[i] = (unsigned char)readback[i];
    status = BHASA_GPU_OK;
cleanup:
    if (device) {
        (void)api.DeviceWaitIdle(device);
        if (mapped) api.UnmapMemory(device, memory);
        if (command_pool) api.DestroyCommandPool(device, command_pool, NULL);
        if (descriptor_pool) api.DestroyDescriptorPool(device, descriptor_pool, NULL);
        if (pipeline) api.DestroyPipeline(device, pipeline, NULL);
        if (pipeline_layout) api.DestroyPipelineLayout(device, pipeline_layout, NULL);
        if (descriptor_layout) api.DestroyDescriptorSetLayout(device, descriptor_layout, NULL);
        if (shader) api.DestroyShaderModule(device, shader, NULL);
        if (buffer) api.DestroyBuffer(device, buffer, NULL);
        if (memory) api.FreeMemory(device, memory, NULL);
        api.DestroyDevice(device, NULL);
    }
    free(readback); free(words); free(name);
    shutdown(&api);
    if (status) fprintf(stderr, "bhasa GPU: Vulkan dispatch failed with status %lld\n", (long long)status);
    return status;
}
