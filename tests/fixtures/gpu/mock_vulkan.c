/* Vulkan host-protocol test double; never loaded by production defaults. */
#include "vendor/vulkan_core.h"
#include <stdlib.h>
#include <string.h>
static int live;
static VkDeviceSize bytes;
static uint32_t *mapped;
static uint32_t push_count, dispatch_count;
static int properties2_enabled, device_create_attempts;
int mock_live_count(void) { return live; }
int mock_device_create_attempts(void) { return device_create_attempts; }
static int fail(const char *stage) { const char *value = getenv("BHASA_MOCK_FAIL"); return value && !strcmp(value, stage); }
static void *allocate(size_t size) { void *p = malloc(size); if (p) ++live; return p; }
static void release(void *p) { if (p) { free(p); --live; } }
#define MAKE(handle) ((*(handle) = (void *)allocate(1)) ? VK_SUCCESS : VK_ERROR_OUT_OF_HOST_MEMORY)
#define IGNORE(value) (void)(value)
VKAPI_ATTR VkResult VKAPI_CALL vkCreateInstance(const VkInstanceCreateInfo *info, const VkAllocationCallbacks *allocator, VkInstance *instance) {
    IGNORE(allocator);
    int enumeration_enabled = 0;
    properties2_enabled = 0;
    for (uint32_t i = 0; i < info->enabledExtensionCount; ++i) {
        if (!strcmp(info->ppEnabledExtensionNames[i], "VK_KHR_get_physical_device_properties2")) properties2_enabled = 1;
        if (!strcmp(info->ppEnabledExtensionNames[i], "VK_KHR_portability_enumeration")) enumeration_enabled = 1;
    }
    if (getenv("BHASA_MOCK_PORTABILITY") && (!(info->flags & 1u) || !enumeration_enabled)) return VK_ERROR_EXTENSION_NOT_PRESENT;
    return MAKE(instance);
}
VKAPI_ATTR void VKAPI_CALL vkDestroyInstance(VkInstance instance, const VkAllocationCallbacks *allocator) { IGNORE(allocator); release(instance); }
VKAPI_ATTR VkResult VKAPI_CALL vkEnumerateInstanceExtensionProperties(const char *layer, uint32_t *count, VkExtensionProperties *properties) {
    IGNORE(layer);
    *count = getenv("BHASA_MOCK_PORTABILITY") ? (getenv("BHASA_MOCK_PROPERTIES2_MISSING") ? 1u : 2u) : 0u;
    if (properties && *count) {
        memset(properties, 0, *count * sizeof(*properties));
        strcpy(properties[0].extensionName, "VK_KHR_portability_enumeration");
        if (*count == 2) strcpy(properties[1].extensionName, "VK_KHR_get_physical_device_properties2");
    }
    return VK_SUCCESS;
}
VKAPI_ATTR VkResult VKAPI_CALL vkEnumeratePhysicalDevices(VkInstance instance, uint32_t *count, VkPhysicalDevice *devices) {
    IGNORE(instance); *count = 1; if (devices) *devices = (void *)(uintptr_t)1; return VK_SUCCESS;
}
VKAPI_ATTR void VKAPI_CALL vkGetPhysicalDeviceProperties(VkPhysicalDevice physical, VkPhysicalDeviceProperties *properties) {
    IGNORE(physical); memset(properties, 0, sizeof(*properties));
    properties->deviceType = getenv("BHASA_MOCK_CPU") ? VK_PHYSICAL_DEVICE_TYPE_CPU : VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU;
    properties->limits.maxStorageBufferRange = 1048576;
    properties->limits.maxComputeWorkGroupCount[0] = 65535;
}
VKAPI_ATTR void VKAPI_CALL vkGetPhysicalDeviceQueueFamilyProperties(VkPhysicalDevice physical, uint32_t *count, VkQueueFamilyProperties *properties) {
    IGNORE(physical); *count = 1;
    if (properties) { memset(properties, 0, sizeof(*properties)); properties->queueCount = 1; properties->queueFlags = VK_QUEUE_COMPUTE_BIT; }
}
VKAPI_ATTR void VKAPI_CALL vkGetPhysicalDeviceMemoryProperties(VkPhysicalDevice physical, VkPhysicalDeviceMemoryProperties *properties) {
    IGNORE(physical); memset(properties, 0, sizeof(*properties)); properties->memoryTypeCount = 1;
    properties->memoryTypes[0].propertyFlags = VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT;
}
VKAPI_ATTR VkResult VKAPI_CALL vkEnumerateDeviceExtensionProperties(VkPhysicalDevice physical, const char *layer, uint32_t *count, VkExtensionProperties *properties) {
    IGNORE(physical); IGNORE(layer); *count = getenv("BHASA_MOCK_PORTABILITY") ? 1u : 0u;
    if (properties && *count) {
        memset(properties, 0, sizeof(*properties));
        strcpy(properties[0].extensionName, "VK_KHR_portability_subset");
    }
    return VK_SUCCESS;
}
VKAPI_ATTR VkResult VKAPI_CALL vkCreateDevice(VkPhysicalDevice physical, const VkDeviceCreateInfo *info, const VkAllocationCallbacks *allocator, VkDevice *device) {
    IGNORE(physical); IGNORE(allocator); ++device_create_attempts;
    if (getenv("BHASA_MOCK_PORTABILITY")) {
        int subset_enabled = 0;
        for (uint32_t i = 0; i < info->enabledExtensionCount; ++i)
            if (!strcmp(info->ppEnabledExtensionNames[i], "VK_KHR_portability_subset")) subset_enabled = 1;
        if (!subset_enabled || !properties2_enabled) return VK_ERROR_EXTENSION_NOT_PRESENT;
    }
    return MAKE(device);
}
VKAPI_ATTR void VKAPI_CALL vkDestroyDevice(VkDevice device, const VkAllocationCallbacks *allocator) { IGNORE(allocator); release(device); }
VKAPI_ATTR void VKAPI_CALL vkGetDeviceQueue(VkDevice device, uint32_t family, uint32_t index, VkQueue *queue) {
    IGNORE(device); IGNORE(family); IGNORE(index); *queue = (void *)(uintptr_t)2;
}
VKAPI_ATTR VkResult VKAPI_CALL vkDeviceWaitIdle(VkDevice device) { IGNORE(device); return VK_SUCCESS; }
VKAPI_ATTR VkResult VKAPI_CALL vkCreateBuffer(VkDevice device, const VkBufferCreateInfo *info, const VkAllocationCallbacks *allocator, VkBuffer *buffer) {
    IGNORE(device); IGNORE(allocator); bytes = info->size; return MAKE(buffer);
}
VKAPI_ATTR void VKAPI_CALL vkDestroyBuffer(VkDevice device, VkBuffer buffer, const VkAllocationCallbacks *allocator) { IGNORE(device); IGNORE(allocator); release(buffer); }
VKAPI_ATTR void VKAPI_CALL vkGetBufferMemoryRequirements(VkDevice device, VkBuffer buffer, VkMemoryRequirements *requirements) {
    IGNORE(device); IGNORE(buffer); requirements->size = bytes; requirements->alignment = 4; requirements->memoryTypeBits = 1;
}
VKAPI_ATTR VkResult VKAPI_CALL vkAllocateMemory(VkDevice device, const VkMemoryAllocateInfo *info, const VkAllocationCallbacks *allocator, VkDeviceMemory *memory) {
    IGNORE(device); IGNORE(allocator); if (fail("allocation")) return VK_ERROR_OUT_OF_DEVICE_MEMORY;
    *memory = (void *)allocate((size_t)info->allocationSize); return *memory ? VK_SUCCESS : VK_ERROR_OUT_OF_HOST_MEMORY;
}
VKAPI_ATTR void VKAPI_CALL vkFreeMemory(VkDevice device, VkDeviceMemory memory, const VkAllocationCallbacks *allocator) { IGNORE(device); IGNORE(allocator); release(memory); }
VKAPI_ATTR VkResult VKAPI_CALL vkBindBufferMemory(VkDevice device, VkBuffer buffer, VkDeviceMemory memory, VkDeviceSize offset) {
    IGNORE(device); IGNORE(buffer); IGNORE(memory); IGNORE(offset); return VK_SUCCESS;
}
VKAPI_ATTR VkResult VKAPI_CALL vkMapMemory(VkDevice device, VkDeviceMemory memory, VkDeviceSize offset, VkDeviceSize size, VkMemoryMapFlags flags, void **pointer) {
    IGNORE(device); IGNORE(offset); IGNORE(size); IGNORE(flags); *pointer = (void *)memory; mapped = *pointer; return VK_SUCCESS;
}
VKAPI_ATTR void VKAPI_CALL vkUnmapMemory(VkDevice device, VkDeviceMemory memory) { IGNORE(device); IGNORE(memory); mapped = NULL; }
VKAPI_ATTR VkResult VKAPI_CALL vkCreateShaderModule(VkDevice device, const VkShaderModuleCreateInfo *info, const VkAllocationCallbacks *allocator, VkShaderModule *shader) {
    IGNORE(device); IGNORE(info); IGNORE(allocator); return MAKE(shader);
}
VKAPI_ATTR void VKAPI_CALL vkDestroyShaderModule(VkDevice device, VkShaderModule shader, const VkAllocationCallbacks *allocator) { IGNORE(device); IGNORE(allocator); release(shader); }
VKAPI_ATTR VkResult VKAPI_CALL vkCreateDescriptorSetLayout(VkDevice device, const VkDescriptorSetLayoutCreateInfo *info, const VkAllocationCallbacks *allocator, VkDescriptorSetLayout *layout) {
    IGNORE(device); IGNORE(info); IGNORE(allocator); return MAKE(layout);
}
VKAPI_ATTR void VKAPI_CALL vkDestroyDescriptorSetLayout(VkDevice device, VkDescriptorSetLayout layout, const VkAllocationCallbacks *allocator) { IGNORE(device); IGNORE(allocator); release(layout); }
VKAPI_ATTR VkResult VKAPI_CALL vkCreatePipelineLayout(VkDevice device, const VkPipelineLayoutCreateInfo *info, const VkAllocationCallbacks *allocator, VkPipelineLayout *layout) {
    IGNORE(device); IGNORE(info); IGNORE(allocator); return MAKE(layout);
}
VKAPI_ATTR void VKAPI_CALL vkDestroyPipelineLayout(VkDevice device, VkPipelineLayout layout, const VkAllocationCallbacks *allocator) { IGNORE(device); IGNORE(allocator); release(layout); }
VKAPI_ATTR VkResult VKAPI_CALL vkCreateComputePipelines(VkDevice device, VkPipelineCache cache, uint32_t count, const VkComputePipelineCreateInfo *info, const VkAllocationCallbacks *allocator, VkPipeline *pipeline) {
    IGNORE(device); IGNORE(cache); IGNORE(count); IGNORE(info); IGNORE(allocator);
    if (fail("pipeline")) return VK_ERROR_INITIALIZATION_FAILED; return MAKE(pipeline);
}
VKAPI_ATTR void VKAPI_CALL vkDestroyPipeline(VkDevice device, VkPipeline pipeline, const VkAllocationCallbacks *allocator) { IGNORE(device); IGNORE(allocator); release(pipeline); }
VKAPI_ATTR VkResult VKAPI_CALL vkCreateDescriptorPool(VkDevice device, const VkDescriptorPoolCreateInfo *info, const VkAllocationCallbacks *allocator, VkDescriptorPool *pool) {
    IGNORE(device); IGNORE(info); IGNORE(allocator); return MAKE(pool);
}
VKAPI_ATTR void VKAPI_CALL vkDestroyDescriptorPool(VkDevice device, VkDescriptorPool pool, const VkAllocationCallbacks *allocator) { IGNORE(device); IGNORE(allocator); release(pool); }
VKAPI_ATTR VkResult VKAPI_CALL vkAllocateDescriptorSets(VkDevice device, const VkDescriptorSetAllocateInfo *info, VkDescriptorSet *set) {
    IGNORE(device); IGNORE(info); *set = (void *)(uintptr_t)3; return VK_SUCCESS;
}
VKAPI_ATTR void VKAPI_CALL vkUpdateDescriptorSets(VkDevice device, uint32_t count, const VkWriteDescriptorSet *writes, uint32_t copies, const VkCopyDescriptorSet *copy) {
    IGNORE(device); IGNORE(count); IGNORE(writes); IGNORE(copies); IGNORE(copy);
}
VKAPI_ATTR VkResult VKAPI_CALL vkCreateCommandPool(VkDevice device, const VkCommandPoolCreateInfo *info, const VkAllocationCallbacks *allocator, VkCommandPool *pool) {
    IGNORE(device); IGNORE(info); IGNORE(allocator); return MAKE(pool);
}
VKAPI_ATTR void VKAPI_CALL vkDestroyCommandPool(VkDevice device, VkCommandPool pool, const VkAllocationCallbacks *allocator) { IGNORE(device); IGNORE(allocator); release(pool); }
VKAPI_ATTR VkResult VKAPI_CALL vkAllocateCommandBuffers(VkDevice device, const VkCommandBufferAllocateInfo *info, VkCommandBuffer *command) {
    IGNORE(device); IGNORE(info); *command = (void *)(uintptr_t)4; return VK_SUCCESS;
}
VKAPI_ATTR VkResult VKAPI_CALL vkBeginCommandBuffer(VkCommandBuffer command, const VkCommandBufferBeginInfo *info) { IGNORE(command); IGNORE(info); return VK_SUCCESS; }
VKAPI_ATTR VkResult VKAPI_CALL vkEndCommandBuffer(VkCommandBuffer command) { IGNORE(command); return VK_SUCCESS; }
VKAPI_ATTR void VKAPI_CALL vkCmdBindPipeline(VkCommandBuffer command, VkPipelineBindPoint point, VkPipeline pipeline) { IGNORE(command); IGNORE(point); IGNORE(pipeline); }
VKAPI_ATTR void VKAPI_CALL vkCmdBindDescriptorSets(VkCommandBuffer command, VkPipelineBindPoint point, VkPipelineLayout layout, uint32_t first, uint32_t count, const VkDescriptorSet *sets, uint32_t dynamic_count, const uint32_t *offsets) {
    IGNORE(command); IGNORE(point); IGNORE(layout); IGNORE(first); IGNORE(count); IGNORE(sets); IGNORE(dynamic_count); IGNORE(offsets);
}
VKAPI_ATTR void VKAPI_CALL vkCmdPushConstants(VkCommandBuffer command, VkPipelineLayout layout, VkShaderStageFlags flags, uint32_t offset, uint32_t size, const void *values) {
    IGNORE(command); IGNORE(layout); IGNORE(flags); IGNORE(offset); IGNORE(size); memcpy(&push_count, values, 4);
}
VKAPI_ATTR void VKAPI_CALL vkCmdDispatch(VkCommandBuffer command, uint32_t x, uint32_t y, uint32_t z) { IGNORE(command); IGNORE(y); IGNORE(z); dispatch_count = x; }
VKAPI_ATTR void VKAPI_CALL vkCmdPipelineBarrier(VkCommandBuffer command, VkPipelineStageFlags source, VkPipelineStageFlags destination, VkDependencyFlags flags, uint32_t count, const VkMemoryBarrier *barriers, uint32_t buffers, const VkBufferMemoryBarrier *buffer_barriers, uint32_t images, const VkImageMemoryBarrier *image_barriers) {
    IGNORE(command); IGNORE(source); IGNORE(destination); IGNORE(flags); IGNORE(count); IGNORE(barriers);
    IGNORE(buffers); IGNORE(buffer_barriers); IGNORE(images); IGNORE(image_barriers);
}
VKAPI_ATTR VkResult VKAPI_CALL vkQueueSubmit(VkQueue queue, uint32_t count, const VkSubmitInfo *submit, VkFence fence) {
    IGNORE(queue); IGNORE(count); IGNORE(submit); IGNORE(fence);
    if (fail("launch") || dispatch_count != push_count || (uint64_t)dispatch_count * 4 != bytes) return VK_ERROR_DEVICE_LOST;
    for (uint32_t i = 0; i < dispatch_count; ++i) mapped[i] = fail("range") ? 256 : ((mapped[i] * 3u + 7u) & 255u);
    return VK_SUCCESS;
}
VKAPI_ATTR VkResult VKAPI_CALL vkQueueWaitIdle(VkQueue queue) { IGNORE(queue); return fail("sync") ? VK_ERROR_DEVICE_LOST : VK_SUCCESS; }
