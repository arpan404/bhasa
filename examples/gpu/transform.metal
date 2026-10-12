#include <metal_stdlib>
using namespace metal;

// gpu_dispatch launches exactly one invocation for each byte in data.
kernel void transform(device uchar *data [[buffer(0)]],
                      uint index [[thread_position_in_grid]]) {
    data[index] = uchar((uint(data[index]) * 3u + 7u) & 255u);
}
