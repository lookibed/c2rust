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
[export]
def take_byte(var value : uint8) : uint8 {
    return value
}
[export]
def take_u64(var value_0 : uint64) : uint64 {
    return value_0
}
[export]
def take_int(var value_1 : int) : int {
    return value_1
}
[export]
def return_byte_literal() : uint8 {
    return uint8(int(171))
}
[export]
def return_u64_literal() : uint64 {
    return uint64(0x100000000uL)
}
[export]
def return_int_literal() : int {
    return int(42)
}
[export]
def typed_literals() : int {
    var byte : uint8 = uint8(0)
    var wide : uint64 = uint64(0)
    var signed_value : int = int(0)
    byte = uint8(int(171))
    wide = uint64(0x100000000uL)
    signed_value = int(42)
    var c2da_fresh0 : int = int(0)
    if (int(take_int(signed_value)) + int(int(take_byte(byte))) + int(int(take_u64(wide))) + int(return_int_literal()) + int(int(return_byte_literal())) + int(int(return_u64_literal())) != int(426)) {
        c2da_fresh0 = int(1)
    }
    return c2da_fresh0
}
