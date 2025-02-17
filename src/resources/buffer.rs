use wgpu::util::{BufferInitDescriptor, DeviceExt};

pub struct BackedBuffer<T> {
    data: Vec<T>,
    buffer: wgpu::Buffer,
    usage: wgpu::BufferUsages,
    version: u32,
}

impl<T: bytemuck::Pod + bytemuck::Zeroable> BackedBuffer<T> {
    pub fn with_capacity(
        device: &wgpu::Device,
        capacity: wgpu::BufferAddress,
        usage: wgpu::BufferUsages,
    ) -> Self {
        let usage = usage | wgpu::BufferUsages::COPY_DST;
        Self {
            data: Vec::with_capacity(capacity as _),
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: None, // Maybe make this accessible
                size: capacity * size_of::<T>() as wgpu::BufferAddress,
                usage,
                mapped_at_creation: false,
            }),
            usage,
            version: 0,
        }
    }

    pub fn with_data(device: &wgpu::Device, data: Vec<T>, usage: wgpu::BufferUsages) -> Self {
        let usage = usage | wgpu::BufferUsages::COPY_DST;
        Self {
            buffer: device.create_buffer_init(&BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&data),
                usage,
            }),
            data,
            usage,
            version: 0,
        }
    }

    pub fn len(&self) -> u32 {
        self.data.len() as _
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn batch<'a>(
        &'a mut self,
        device: &'a wgpu::Device,
        queue: &'a wgpu::Queue,
    ) -> Batch<'a, T> {
        Batch::new(self, device, queue)
    }

    #[allow(unused)]
    pub fn batch_indexed<'a>(
        &'a mut self,
        device: &'a wgpu::Device,
        queue: &'a wgpu::Queue,
        indices: &'a mut BackedBuffer<u32>,
    ) -> IndexedBatch<'a, T> {
        IndexedBatch::new(device, queue, self, indices)
    }

    #[allow(unused)]
    pub fn slice(&self) -> wgpu::BufferSlice<'_> {
        self.buffer.slice(..)
    }

    pub fn update(&mut self, queue: &wgpu::Queue, mut f: impl FnMut(&mut [T])) {
        f(&mut self.data);
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.data));
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }
}

pub struct Batch<'a, T: bytemuck::Pod + bytemuck::Zeroable> {
    vertices: &'a mut BackedBuffer<T>,
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    start_vertex: usize,
}

impl<'a, T: bytemuck::Pod + bytemuck::Zeroable> Batch<'a, T> {
    pub fn new(
        vertices: &'a mut BackedBuffer<T>,
        device: &'a wgpu::Device,
        queue: &'a wgpu::Queue,
    ) -> Self {
        Self {
            start_vertex: vertices.len() as _,
            vertices,
            device,
            queue,
        }
    }

    pub fn push(&mut self, value: T) -> &mut Self {
        self.vertices.data.push(value);
        self
    }
}

impl<'a, T: bytemuck::Pod + bytemuck::Zeroable> Drop for Batch<'a, T> {
    fn drop(&mut self) {
        if self.start_vertex < self.vertices.data.len() {
            let size = (self.vertices.data.capacity() * size_of::<T>()) as wgpu::BufferAddress;
            if size > self.vertices.buffer.size() {
                self.vertices.buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size,
                    usage: self.vertices.usage,
                    mapped_at_creation: false,
                });
                self.queue.write_buffer(
                    &self.vertices.buffer,
                    0,
                    bytemuck::cast_slice(&self.vertices.data),
                );
                self.vertices.version += 1;
            } else {
                let offset = (self.start_vertex * size_of::<T>()) as wgpu::BufferAddress;
                self.queue.write_buffer(
                    &self.vertices.buffer,
                    offset,
                    bytemuck::cast_slice(&self.vertices.data[self.start_vertex..]),
                );
            }
        }
    }
}

pub struct IndexedBatch<'a, T: bytemuck::Pod + bytemuck::Zeroable> {
    indices: &'a mut BackedBuffer<u32>,
    start_index: usize,
    batch: Batch<'a, T>,
}

impl<'a, T: bytemuck::Pod + bytemuck::Zeroable> IndexedBatch<'a, T> {
    pub fn new(
        device: &'a wgpu::Device,
        queue: &'a wgpu::Queue,
        vertices: &'a mut BackedBuffer<T>,
        indices: &'a mut BackedBuffer<u32>,
    ) -> Self {
        Self {
            start_index: indices.data.len(),
            batch: Batch::new(vertices, device, queue),
            indices,
        }
    }

    #[allow(unused)]
    pub fn vertex(&mut self, v: T) -> &mut Self {
        self.indices.data.push(self.batch.vertices.len());
        self.batch.push(v);
        self
    }

    #[allow(unused)]
    pub fn line(&mut self, a: T, b: T) -> &mut Self {
        self.vertex(a);
        self.vertex(b);
        self
    }
}

impl<'a, T: bytemuck::Pod + bytemuck::Zeroable> Drop for IndexedBatch<'a, T> {
    fn drop(&mut self) {
        if self.start_index < self.indices.data.len() {
            let size = (self.indices.data.capacity() * size_of::<T>()) as wgpu::BufferAddress;
            if size > self.indices.buffer.size() {
                self.indices.buffer = self.batch.device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size,
                    usage: self.indices.usage,
                    mapped_at_creation: false,
                });
                self.batch.queue.write_buffer(
                    &self.indices.buffer,
                    0,
                    bytemuck::cast_slice(&self.indices.data),
                );
                self.indices.version += 1;
            } else {
                let offset = (self.start_index * size_of::<T>()) as wgpu::BufferAddress;
                self.batch.queue.write_buffer(
                    &self.indices.buffer,
                    offset,
                    bytemuck::cast_slice(&self.indices.data[self.start_index..]),
                );
            }
        }
    }
}
