options gen2

var c2da_rt_heap : array<uint8>

var c2da_rt_next : uint64 = 0x0

var c2da_rt_alloc_addrs : array<uint64>

var c2da_rt_alloc_sizes : array<uint64>

var c2da_rt_alloc_live : array<bool>

def c2da_rt_init_heap() {
    if (length(c2da_rt_heap) == 0) {
        reserve(c2da_rt_heap, int(0x4000000))
        resize(c2da_rt_heap, 1)
    }
}
def c2da_rt_reset() {
    c2da_rt_next = 0x0
    resize(c2da_rt_alloc_addrs, 0)
    resize(c2da_rt_alloc_sizes, 0)
    resize(c2da_rt_alloc_live, 0)
}
def c2da_rt_malloc(size : uint64) : uint64 {
    c2da_rt_init_heap()
    if (size == 0x0) {
        return 0x0
    }
    var start : uint64 = c2da_rt_next
    var end : uint64 = start + size
    if (end > 0x4000000) {
        return 0x0
    }
    if (int(end) > length(c2da_rt_heap)) {
        resize(c2da_rt_heap, int(end))
    }
    c2da_rt_next = end
    var address : uint64 = unsafe(intptr(unsafe(addr(c2da_rt_heap[int(start)]))))
    var record : uint64 = uint64(length(c2da_rt_alloc_addrs))
    resize(c2da_rt_alloc_addrs, int(record + 0x1))
    resize(c2da_rt_alloc_sizes, int(record + 0x1))
    resize(c2da_rt_alloc_live, int(record + 0x1))
    c2da_rt_alloc_addrs[int(record)] = address
    c2da_rt_alloc_sizes[int(record)] = size
    c2da_rt_alloc_live[int(record)] = true
    return address
}
def c2da_rt_free(address : uint64) {
    var i : uint64 = 0x0
    while (int(i) < length(c2da_rt_alloc_addrs)) {
        if (c2da_rt_alloc_live[int(i)] && c2da_rt_alloc_addrs[int(i)] == address) {
            c2da_rt_alloc_live[int(i)] = false
            return
        }
        i = i + 0x1
    }
}
def c2da_rt_realloc(address : uint64; size : uint64) : uint64 {
    if (address == 0x0) {
        return c2da_rt_malloc(size)
    }
    if (size == 0x0) {
        c2da_rt_free(address)
        return 0x0
    }
    var i : uint64 = 0x0
    var old_size : uint64 = 0x0
    var found : bool = false
    while (int(i) < length(c2da_rt_alloc_addrs)) {
        if (c2da_rt_alloc_live[int(i)] && c2da_rt_alloc_addrs[int(i)] == address) {
            old_size = c2da_rt_alloc_sizes[int(i)]
            found = true
        }
        i = i + 0x1
    }
    if (found == false) {
        return 0x0
    }
    var replacement : uint64 = c2da_rt_malloc(size)
    if (replacement == 0x0) {
        return 0x0
    }
    var copy_size : uint64 = 0x0
    if (old_size < size) {
        copy_size = old_size
    } else {
        copy_size = size
    }
    c2da_rt_memcpy(replacement, address, copy_size)
    c2da_rt_free(address)
    return replacement
}
def c2da_rt_memset(dst : uint64; value : uint8; count : uint64) : uint64 {
    var i : uint64 = 0x0
    while (i < count) {
        unsafe(unsafe(unsafe(reinterpret<uint8?>(dst)))[int(i)]) = value
        i = i + 0x1
    }
    return dst
}
def c2da_rt_calloc(count : uint64; size : uint64) : uint64 {
    var total : uint64 = count * size
    if (size != 0x0 && total / size != count) {
        return 0x0
    }
    var address : uint64 = c2da_rt_malloc(total)
    if (address != 0x0) {
        c2da_rt_memset(address, uint8(0x0), total)
    }
    return address
}
def c2da_rt_memcpy(dst : uint64; src : uint64; count : uint64) : uint64 {
    var i : uint64 = 0x0
    while (i < count) {
        unsafe(unsafe(unsafe(reinterpret<uint8?>(dst)))[int(i)]) = unsafe(unsafe(unsafe(reinterpret<uint8?>(src)))[int(i)])
        i = i + 0x1
    }
    return dst
}
def c2da_rt_memcmp(left : uint64; right : uint64; count : uint64) : int {
    var i : uint64 = 0x0
    while (i < count) {
        if (uint(unsafe(unsafe(unsafe(reinterpret<uint8?>(left)))[int(i)])) < uint(unsafe(unsafe(unsafe(reinterpret<uint8?>(right)))[int(i)]))) {
            return -1
        }
        if (uint(unsafe(unsafe(unsafe(reinterpret<uint8?>(left)))[int(i)])) > uint(unsafe(unsafe(unsafe(reinterpret<uint8?>(right)))[int(i)]))) {
            return 1
        }
        i = i + 0x1
    }
    return 0
}
def c2da_rt_memmove(dst : uint64; src : uint64; count : uint64) : uint64 {
    if (dst < src) {
        var i : uint64 = 0x0
        while (i < count) {
            unsafe(unsafe(unsafe(reinterpret<uint8?>(dst)))[int(i)]) = unsafe(unsafe(unsafe(reinterpret<uint8?>(src)))[int(i)])
            i = i + 0x1
        }
    } else {
        var i : uint64 = count
        while (i > 0x0) {
            i = i - 0x1
            unsafe(unsafe(unsafe(reinterpret<uint8?>(dst)))[int(i)]) = unsafe(unsafe(unsafe(reinterpret<uint8?>(src)))[int(i)])
        }
    }
    return dst
}
def c2da_rt_memchr(src : uint64; value : uint8; count : uint64) : uint64 {
    var i : uint64 = 0x0
    while (i < count) {
        if (unsafe(unsafe(unsafe(reinterpret<uint8?>(src)))[int(i)]) == value) {
            return src + i
        }
        i = i + 0x1
    }
    return 0x0
}
struct C2daVaArg {
    tag : int
    i64 : int64
    f64 : double
    raw : uint64
}
struct C2daVaCursor {
    index : int
}
def c2da_min_int(a : int; b : int) : int {
    if (a < b) {
        return a
    } else {
        return b
    }
}
def c2da_max_int(a : int; b : int) : int {
    if (a > b) {
        return a
    } else {
        return b
    }
}
def c2da_min_uint(a : uint; b : uint) : uint {
    if (a < b) {
        return a
    } else {
        return b
    }
}
def c2da_max_uint(a : uint; b : uint) : uint {
    if (a > b) {
        return a
    } else {
        return b
    }
}
def c2da_min_int64(a : int64; b : int64) : int64 {
    if (a < b) {
        return a
    } else {
        return b
    }
}
def c2da_max_int64(a : int64; b : int64) : int64 {
    if (a > b) {
        return a
    } else {
        return b
    }
}
def c2da_min_uint64(a : uint64; b : uint64) : uint64 {
    if (a < b) {
        return a
    } else {
        return b
    }
}
def c2da_max_uint64(a : uint64; b : uint64) : uint64 {
    if (a > b) {
        return a
    } else {
        return b
    }
}
def c2da_clip_uint(v : int) : uint {
    if (v < 0) {
        return 0x0
    }
    if (v > 255) {
        return 0xff
    }
    return uint(v)
}
def c2da_bool_to_uint(v : bool) : uint {
    if (v == true) {
        return 0x1
    } else {
        return 0x0
    }
}
def c2da___assert_fail(expr : void?; file : void?; line : uint; func : void?) {
}
typedef uint8_t = uint8
typedef uint16_t = uint16
typedef uint32_t = uint
typedef size_t = uint64
struct nested_inner {
    count : uint
    flags : uint16
}
struct nested_outer {
    tag : uint8
    inner : nested_inner
}
def c2da___bswap_16(var c2da___bsx : uint16) : uint16 {
    return uint16(int(c2da___bsx) >> int(8) & int(255) | (int(c2da___bsx) & int(255)) << int(8))
}
def c2da___bswap_32(var c2da___bsx_0 : uint) : uint {
    return (c2da___bsx_0 & uint(0xff000000)) >> uint(int(24)) | (c2da___bsx_0 & uint(0xff0000)) >> uint(int(8)) | (c2da___bsx_0 & uint(0xff00)) << uint(int(8)) | (c2da___bsx_0 & uint(0xff)) << uint(int(24))
}
def c2da___bswap_64(var c2da___bsx_1 : uint64) : uint64 {
    return (c2da___bsx_1 & uint64(0xff00000000000000uL)) >> uint64(int(56)) | (c2da___bsx_1 & uint64(0xff000000000000uL)) >> uint64(int(40)) | (c2da___bsx_1 & uint64(0xff0000000000uL)) >> uint64(int(24)) | (c2da___bsx_1 & uint64(0xff00000000uL)) >> uint64(int(8)) | (c2da___bsx_1 & uint64(0xff000000)) << uint64(int(8)) | (c2da___bsx_1 & uint64(0xff0000)) << uint64(int(24)) | (c2da___bsx_1 & uint64(0xff00)) << uint64(int(40)) | (c2da___bsx_1 & uint64(0xff)) << uint64(int(56))
}
def c2da___uint16_identity(var c2da___x : uint16) : uint16 {
    return c2da___x
}
def c2da___uint32_identity(var c2da___x_0 : uint) : uint {
    return c2da___x_0
}
def c2da___uint64_identity(var c2da___x_1 : uint64) : uint64 {
    return c2da___x_1
}
[export]
def nested_raw_aggregate_place_runtime() : int {
    var object : nested_outer? = null
    var failed : int = int(0)
    object = unsafe(unsafe(reinterpret<nested_outer?>(unsafe(unsafe(reinterpret<uint8?>(c2da_rt_calloc(uint64(1), uint64(12))))))))
    unsafe(unsafe(unsafe(reinterpret<uint?>(unsafe(unsafe(reinterpret<uint64>(object))))))[1]) = uint(0x10203040)
    uint(0x10203040)
    unsafe(unsafe(unsafe(reinterpret<uint16?>(unsafe(unsafe(reinterpret<uint64>(object))))))[4]) = uint16(uint(0x7))
    uint16(uint(0x7))
    var c2da_fresh0 : int = int(0)
    if (unsafe(unsafe(unsafe(reinterpret<uint?>(unsafe(unsafe(reinterpret<uint64>(object))))))[1]) != uint(0x10203040)) {
        c2da_fresh0 = int(1)
    }
    var c2da_fresh2 : int = 1
    if (!(c2da_fresh0 != 0)) {
        var c2da_fresh1 : int = int(0)
        if (uint(unsafe(unsafe(unsafe(reinterpret<uint16?>(unsafe(unsafe(reinterpret<uint64>(object))))))[4])) != uint(0x7)) {
            c2da_fresh1 = int(1)
        }
        if (!(c2da_fresh1 != 0)) {
            c2da_fresh2 = 0
        }
    }
    failed = c2da_fresh2
    c2da_rt_free(unsafe(unsafe(reinterpret<uint64>(unsafe(unsafe(reinterpret<uint8?>(object)))))))
    return failed
}
