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
def for_sum(var n : int) : int {
    var s : int = int(0)
    var i : int = int(0)
    s = int(0)
    i = int(0)
    label 0:
    var c2da_fresh0 : int = int(0)
    if (i < n) {
        c2da_fresh0 = int(1)
    }
    if (c2da_fresh0 != 0) {
        goto label 2
    }
    label 1:
    return s
    label 2:
    var c2da_fresh1 : int = int(0)
    if (i == int(3)) {
        c2da_fresh1 = int(1)
    }
    if (c2da_fresh1 != 0) {
        goto label 5
    }
    var c2da_fresh2 : int = int(0)
    if (i == int(7)) {
        c2da_fresh2 = int(1)
    }
    if (c2da_fresh2 != 0) {
        goto label 4
    }
    s = int(s + i)
    label 3:
    var c2da_postinc : int = i
    i = int(i + 1)
    c2da_postinc
    goto label 0
    label 4:
    goto label 1
    label 5:
    goto label 3
    panic("unreachable: fell out of a translated control-flow graph")
}
[export]
def while_sum(var n_0 : int) : int {
    var s_0 : int = int(0)
    var i_0 : int = int(0)
    s_0 = int(0)
    i_0 = int(0)
    label 0:
    var c2da_fresh3 : int = int(0)
    if (i_0 < n_0) {
        c2da_fresh3 = int(1)
    }
    if (c2da_fresh3 != 0) {
        goto label 1
    }
    return s_0
    label 1:
    s_0 = int(s_0 + i_0)
    var c2da_postinc_0 : int = i_0
    i_0 = int(i_0 + 1)
    c2da_postinc_0
    goto label 0
    panic("unreachable: fell out of a translated control-flow graph")
}
[export]
def do_while_continue(var n_1 : int) : int {
    var s_1 : int = int(0)
    s_1 = int(0)
    label 0:
    s_1 = int(s_1 + n_1)
    var c2da_postinc_1 : int = n_1
    n_1 = int(n_1 - 1)
    c2da_postinc_1
    var c2da_fresh4 : int = int(0)
    if (n_1 == int(2)) {
        c2da_fresh4 = int(1)
    }
    if (c2da_fresh4 != 0) {
        goto label 2
    }
    label 1:
    var c2da_fresh5 : int = int(0)
    if (n_1 > int(0)) {
        c2da_fresh5 = int(1)
    }
    if (c2da_fresh5 != 0) {
        goto label 0
    }
    return s_1
    label 2:
    goto label 1
    panic("unreachable: fell out of a translated control-flow graph")
}
[export]
def nested() : int {
    var c : int = int(0)
    var i_1 : int = int(0)
    var j : int = int(0)
    c = int(0)
    i_1 = int(0)
    label 0:
    var c2da_fresh6 : int = int(0)
    if (i_1 < int(5)) {
        c2da_fresh6 = int(1)
    }
    if (c2da_fresh6 != 0) {
        goto label 1
    }
    return c
    label 1:
    j = int(0)
    label 2:
    var c2da_fresh7 : int = int(0)
    if (j < int(5)) {
        c2da_fresh7 = int(1)
    }
    if (c2da_fresh7 != 0) {
        goto label 4
    }
    label 3:
    var c2da_postinc_4 : int = i_1
    i_1 = int(i_1 + 1)
    c2da_postinc_4
    goto label 0
    label 4:
    var c2da_fresh8 : int = int(0)
    if (j > i_1) {
        c2da_fresh8 = int(1)
    }
    if (c2da_fresh8 != 0) {
        goto label 7
    }
    if ((i_1 + j) % int(2) != 0) {
        goto label 6
    }
    var c2da_postinc_2 : int = c
    c = int(c + 1)
    c2da_postinc_2
    label 5:
    var c2da_postinc_3 : int = j
    j = int(j + 1)
    c2da_postinc_3
    goto label 2
    label 6:
    goto label 5
    label 7:
    goto label 3
    panic("unreachable: fell out of a translated control-flow graph")
}
[export]
def early_return(var n_2 : int) : int {
    var i_2 : int = int(0)
    i_2 = int(0)
    label 0:
    var c2da_fresh9 : int = int(0)
    if (i_2 * i_2 > n_2) {
        c2da_fresh9 = int(1)
    }
    if (c2da_fresh9 != 0) {
        goto label 1
    }
    var c2da_postinc_5 : int = i_2
    i_2 = int(i_2 + 1)
    c2da_postinc_5
    goto label 0
    label 1:
    return i_2
}
[export]
def empty_body(var n_3 : int) : int {
    var i_3 : int = int(0)
    i_3 = int(0)
    label 0:
    var c2da_fresh10 : int = int(0)
    if (i_3 < n_3) {
        c2da_fresh10 = int(1)
    }
    if (c2da_fresh10 != 0) {
        goto label 1
    }
    return i_3
    label 1:
    var c2da_postinc_6 : int = i_3
    i_3 = int(i_3 + 1)
    c2da_postinc_6
    goto label 0
    panic("unreachable: fell out of a translated control-flow graph")
}
[export]
def forever_break() : int {
    var k : int = int(0)
    k = int(0)
    label 0:
    k = int(k + int(3))
    var c2da_fresh11 : int = int(0)
    if (k > int(10)) {
        c2da_fresh11 = int(1)
    }
    if (c2da_fresh11 != 0) {
        goto label 1
    }
    goto label 0
    label 1:
    return k
}
[export]
def comma_for() : int {
    var i_4 : int = int(0)
    var j_0 : int = int(0)
    var s_2 : int = int(0)
    s_2 = int(0)
    i_4 = int(0)
    j_0 = int(10)
    label 0:
    var c2da_fresh12 : int = int(0)
    if (i_4 < j_0) {
        c2da_fresh12 = int(1)
    }
    if (c2da_fresh12 != 0) {
        goto label 1
    }
    return s_2
    label 1:
    s_2 = int(s_2 + (j_0 - i_4))
    var c2da_postinc_7 : int = i_4
    i_4 = int(i_4 + 1)
    c2da_postinc_7
    var c2da_postinc_8 : int = j_0
    j_0 = int(j_0 - 1)
    c2da_postinc_8
    goto label 0
    panic("unreachable: fell out of a translated control-flow graph")
}
[export]
def loops_runtime() : int {
    var c2da_fresh13 : int = int(0)
    if (int(for_sum(int(10))) != int(18)) {
        c2da_fresh13 = int(1)
    }
    if (c2da_fresh13 != 0) {
        goto label 7
    }
    var c2da_fresh14 : int = int(0)
    if (int(while_sum(int(10))) != int(45)) {
        c2da_fresh14 = int(1)
    }
    if (c2da_fresh14 != 0) {
        goto label 6
    }
    var c2da_fresh15 : int = int(0)
    if (int(do_while_continue(int(4))) != int(10)) {
        c2da_fresh15 = int(1)
    }
    if (c2da_fresh15 != 0) {
        goto label 5
    }
    var c2da_fresh16 : int = int(0)
    if (int(nested()) != int(9)) {
        c2da_fresh16 = int(1)
    }
    if (c2da_fresh16 != 0) {
        goto label 4
    }
    var c2da_fresh17 : int = int(0)
    if (int(early_return(int(10))) != int(4)) {
        c2da_fresh17 = int(1)
    }
    if (c2da_fresh17 != 0) {
        goto label 3
    }
    var c2da_fresh18 : int = int(0)
    if (int(empty_body(int(7))) != int(7)) {
        c2da_fresh18 = int(1)
    }
    if (c2da_fresh18 != 0) {
        goto label 2
    }
    var c2da_fresh19 : int = int(0)
    if (int(forever_break()) != int(12)) {
        c2da_fresh19 = int(1)
    }
    if (c2da_fresh19 != 0) {
        goto label 1
    }
    var c2da_fresh20 : int = int(0)
    if (int(comma_for()) != int(30)) {
        c2da_fresh20 = int(1)
    }
    if (c2da_fresh20 != 0) {
        goto label 0
    }
    return int(0)
    label 0:
    return int(8)
    label 1:
    return int(7)
    label 2:
    return int(6)
    label 3:
    return int(5)
    label 4:
    return int(4)
    label 5:
    return int(3)
    label 6:
    return int(2)
    label 7:
    return int(1)
}
