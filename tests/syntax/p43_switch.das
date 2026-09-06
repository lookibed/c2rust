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
def classify(var x : int) : int {
    var r : int = int(0)
    r = int(0)
    if (x == int(1)) {
        goto label 2
    } elif (x == int(2)) {
        goto label 3
    } elif (x == int(3)) {
        goto label 4
    } elif (x == int(4)) {
        goto label 0
    }
    r = int(r + int(1000))
    label 0:
    r = int(r + int(40))
    label 1:
    return r
    label 2:
    r = int(r + int(10))
    label 3:
    r = int(r + int(20))
    goto label 1
    label 4:
    r = int(r + int(30))
    goto label 1
    panic("unreachable: fell out of a translated control-flow graph")
}
[export]
def in_loop() : int {
    var c2da_fresh1 : int
    var acc : int = int(0)
    var i : int = int(0)
    acc = int(0)
    i = int(0)
    label 0:
    var c2da_fresh0 : int = int(0)
    if (i < int(6)) {
        c2da_fresh0 = int(1)
    }
    if (c2da_fresh0 != 0) {
        goto label 1
    }
    return acc
    label 1:
    c2da_fresh1 = i % int(3)
    if (c2da_fresh1 == int(0)) {
        goto label 4
    } elif (c2da_fresh1 == int(1)) {
        goto label 5
    }
    acc = int(acc + int(100))
    label 2:
    acc = int(acc + int(1))
    label 3:
    var c2da_postinc : int = i
    i = int(i + 1)
    c2da_postinc
    goto label 0
    label 4:
    goto label 3
    label 5:
    acc = int(acc + i)
    goto label 2
    panic("unreachable: fell out of a translated control-flow graph")
}
[export]
def nested_switch(var a : int; var b : int) : int {
    if (a == int(0)) {
        goto label 0
    } elif (a == int(1)) {
        goto label 2
    }
    return int(5)
    label 0:
    if (b == int(0)) {
        goto label 1
    }
    return int(2)
    label 1:
    return int(1)
    label 2:
    if (b != 0) {
        goto label 3
    }
    return int(4)
    label 3:
    return int(3)
}
[export]
def char_switch(var c : int8) : int {
    var c2da_fresh2 : int
    c2da_fresh2 = int(c)
    if (c2da_fresh2 == int(97)) {
        goto label 0
    } elif (c2da_fresh2 == int(101)) {
        goto label 1
    } elif (c2da_fresh2 == int(105)) {
        goto label 2
    }
    return int(0)
    label 0:
    label 1:
    label 2:
    return int(1)
}
[export]
def no_default_no_match(var x_0 : int) : int {
    var r_0 : int = int(0)
    r_0 = int(7)
    if (x_0 == int(1)) {
        goto label 1
    }
    label 0:
    return r_0
    label 1:
    r_0 = int(1)
    goto label 0
    panic("unreachable: fell out of a translated control-flow graph")
}
[export]
def switch_runtime() : int {
    var c2da_fresh3 : int = int(0)
    if (int(classify(int(1))) != int(30)) {
        c2da_fresh3 = int(1)
    }
    if (c2da_fresh3 != 0) {
        goto label 9
    }
    var c2da_fresh4 : int = int(0)
    if (int(classify(int(2))) != int(20)) {
        c2da_fresh4 = int(1)
    }
    if (c2da_fresh4 != 0) {
        goto label 8
    }
    var c2da_fresh5 : int = int(0)
    if (int(classify(int(3))) != int(30)) {
        c2da_fresh5 = int(1)
    }
    if (c2da_fresh5 != 0) {
        goto label 7
    }
    var c2da_fresh6 : int = int(0)
    if (int(classify(int(4))) != int(40)) {
        c2da_fresh6 = int(1)
    }
    if (c2da_fresh6 != 0) {
        goto label 6
    }
    var c2da_fresh7 : int = int(0)
    if (int(classify(int(9))) != int(1040)) {
        c2da_fresh7 = int(1)
    }
    if (c2da_fresh7 != 0) {
        goto label 5
    }
    var c2da_fresh8 : int = int(0)
    if (int(in_loop()) != int(209)) {
        c2da_fresh8 = int(1)
    }
    if (c2da_fresh8 != 0) {
        goto label 4
    }
    var c2da_fresh9 : int = int(0)
    if (int(nested_switch(int(0), int(0))) != int(1)) {
        c2da_fresh9 = int(1)
    }
    var c2da_fresh11 : int = 1
    if (!(c2da_fresh9 != 0)) {
        var c2da_fresh10 : int = int(0)
        if (int(nested_switch(int(0), int(5))) != int(2)) {
            c2da_fresh10 = int(1)
        }
        if (!(c2da_fresh10 != 0)) {
            c2da_fresh11 = 0
        }
    }
    if (c2da_fresh11 != 0) {
        goto label 3
    }
    var c2da_fresh12 : int = int(0)
    if (int(nested_switch(int(1), int(1))) != int(3)) {
        c2da_fresh12 = int(1)
    }
    var c2da_fresh14 : int = 1
    if (!(c2da_fresh12 != 0)) {
        var c2da_fresh13 : int = int(0)
        if (int(nested_switch(int(1), int(0))) != int(4)) {
            c2da_fresh13 = int(1)
        }
        if (!(c2da_fresh13 != 0)) {
            c2da_fresh14 = 0
        }
    }
    var c2da_fresh16 : int = 1
    if (!(c2da_fresh14 != 0)) {
        var c2da_fresh15 : int = int(0)
        if (int(nested_switch(int(7), int(0))) != int(5)) {
            c2da_fresh15 = int(1)
        }
        if (!(c2da_fresh15 != 0)) {
            c2da_fresh16 = 0
        }
    }
    if (c2da_fresh16 != 0) {
        goto label 2
    }
    var c2da_fresh17 : int = int(0)
    if (int(char_switch(int8(101))) != int(1)) {
        c2da_fresh17 = int(1)
    }
    var c2da_fresh19 : int = 1
    if (!(c2da_fresh17 != 0)) {
        var c2da_fresh18 : int = int(0)
        if (int(char_switch(int8(122))) != int(0)) {
            c2da_fresh18 = int(1)
        }
        if (!(c2da_fresh18 != 0)) {
            c2da_fresh19 = 0
        }
    }
    if (c2da_fresh19 != 0) {
        goto label 1
    }
    var c2da_fresh20 : int = int(0)
    if (int(no_default_no_match(int(1))) != int(1)) {
        c2da_fresh20 = int(1)
    }
    var c2da_fresh22 : int = 1
    if (!(c2da_fresh20 != 0)) {
        var c2da_fresh21 : int = int(0)
        if (int(no_default_no_match(int(2))) != int(7)) {
            c2da_fresh21 = int(1)
        }
        if (!(c2da_fresh21 != 0)) {
            c2da_fresh22 = 0
        }
    }
    if (c2da_fresh22 != 0) {
        goto label 0
    }
    return int(0)
    label 0:
    return int(10)
    label 1:
    return int(9)
    label 2:
    return int(8)
    label 3:
    return int(7)
    label 4:
    return int(6)
    label 5:
    return int(5)
    label 6:
    return int(4)
    label 7:
    return int(3)
    label 8:
    return int(2)
    label 9:
    return int(1)
}
