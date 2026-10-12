extern "C" __global__ void transform(unsigned char *data, unsigned int count) {
    unsigned int index = blockIdx.x * blockDim.x + threadIdx.x;
    if (index < count) data[index] = (unsigned char)((data[index] * 3u + 7u) & 255u);
}
