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
def dbl_literal() : int {
    var d : double = 0.0lf
    d = 1000000000000.0lf
    var c2da_fresh0 : int = int(0)
    if (int64(d) == int64(0xe8d4a51000uL)) {
        c2da_fresh0 = int(1)
    }
    return c2da_fresh0
}
[export]
def dbl_exact() : int {
    var d_0 : double = 0.0lf
    d_0 = 16777217.0lf
    var c2da_fresh1 : int = int(0)
    if (int64(d_0) == int64(16777217)) {
        c2da_fresh1 = int(1)
    }
    return c2da_fresh1
}
[export]
def dbl_third() : int {
    var d_1 : double = 0.0lf
    d_1 = 1.0lf / 3.0lf
    var c2da_fresh2 : int = int(0)
    if (d_1 > 0.33333333lf) {
        c2da_fresh2 = int(1)
    }
    var c2da_fresh4 : int = 0
    if (c2da_fresh2 != 0) {
        var c2da_fresh3 : int = int(0)
        if (d_1 < 0.33333334lf) {
            c2da_fresh3 = int(1)
        }
        if (c2da_fresh3 != 0) {
            c2da_fresh4 = 1
        }
    }
    var c2da_fresh6 : int = 0
    if (c2da_fresh4 != 0) {
        var c2da_fresh5 : int = int(0)
        if (d_1 * 3.0lf == 1.0lf) {
            c2da_fresh5 = int(1)
        }
        if (c2da_fresh5 != 0) {
            c2da_fresh6 = 1
        }
    }
    return c2da_fresh6
}
[export]
def cmp_zero() : int {
    var d_2 : double = 0.0lf
    var a : int = int(0)
    var b : int = int(0)
    var c : int = int(0)
    var e : int = int(0)
    var n : double = 0.0lf
    var f : int = int(0)
    d_2 = 0.5lf
    var c2da_fresh7 : int = int(0)
    if (d_2 == double(int(0))) {
        c2da_fresh7 = int(1)
    }
    a = c2da_fresh7
    var c2da_fresh8 : int = int(0)
    if (d_2 != double(int(0))) {
        c2da_fresh8 = int(1)
    }
    b = c2da_fresh8
    var c2da_fresh9 : int = int(0)
    if (d_2 != 0.0lf) {
        c2da_fresh9 = int(1)
    } else {
        c2da_fresh9 = int(0)
    }
    c = c2da_fresh9
    e = int(0)
    if (d_2 != 0.0lf) {
        goto label 3
    }
    label 0:
    n = -0.5lf
    f = int(0)
    if (n != 0.0lf) {
        goto label 2
    }
    label 1:
    return a * int(10000) + b * int(1000) + c * int(100) + e * int(10) + f
    label 2:
    f = int(1)
    goto label 1
    label 3:
    e = int(1)
    goto label 0
    panic("unreachable: fell out of a translated control-flow graph")
}
[export]
def flt() : int {
    var f_0 : float = 0.0
    var h : float = 0.0
    f_0 = 1.5
    f_0 = float(f_0 * 2.0)
    h = 0.10000000149011612
    var c2da_fresh10 : int = int(0)
    if (f_0 == 3.0) {
        c2da_fresh10 = int(1)
    }
    var c2da_fresh12 : int = 0
    if (c2da_fresh10 != 0) {
        var c2da_fresh11 : int = int(0)
        if (h > 0.09000000357627869) {
            c2da_fresh11 = int(1)
        }
        if (c2da_fresh11 != 0) {
            c2da_fresh12 = 1
        }
    }
    var c2da_fresh14 : int = 0
    if (c2da_fresh12 != 0) {
        var c2da_fresh13 : int = int(0)
        if (h < 0.10999999940395355) {
            c2da_fresh13 = int(1)
        }
        if (c2da_fresh13 != 0) {
            c2da_fresh14 = 1
        }
    }
    return c2da_fresh14
}
[export]
def not_dbl() : int {
    var d_3 : double = 0.0lf
    var e_0 : double = 0.0lf
    d_3 = 0.0lf
    e_0 = 2.0lf
    var c2da_fresh15 : int = int(0)
    if (d_3 == 0.0lf) {
        c2da_fresh15 = int(1)
    }
    var c2da_fresh16 : int = int(0)
    if (e_0 != 0.0lf && int(1) != 0) {
        c2da_fresh16 = int(1)
    }
    return c2da_fresh15 * int(10) + c2da_fresh16
}
[export]
def conv() : int {
    var d_4 : double = 0.0lf
    var i : int = int(0)
    var g : float = 0.0
    d_4 = -2.7lf
    i = int(d_4)
    g = float(float(7)) / float(int(2))
    return i * int(100) + int(g * float(int(10)))
}
[export]
def while_dbl() : int {
    var x : double = 0.0lf
    var n_0 : int = int(0)
    x = 100.0lf
    n_0 = int(0)
    label 0:
    var c2da_fresh17 : int = int(0)
    if (x > 1.0lf) {
        c2da_fresh17 = int(1)
    }
    if (c2da_fresh17 != 0) {
        goto label 1
    }
    return n_0
    label 1:
    x = double(x / 2.0lf)
    var c2da_postinc : int = n_0
    n_0 = int(n_0 + 1)
    c2da_postinc
    goto label 0
    panic("unreachable: fell out of a translated control-flow graph")
}
[export]
def mixed() : int {
    var i_0 : int = int(0)
    var d_5 : double = 0.0lf
    var e_1 : double = 0.0lf
    i_0 = int(7)
    d_5 = double(i_0 / int(2))
    e_1 = double(i_0) / 2.0lf
    var c2da_fresh18 : int = int(0)
    if (d_5 == 3.0lf) {
        c2da_fresh18 = int(1)
    }
    var c2da_fresh19 : int = int(0)
    if (e_1 == 3.5lf) {
        c2da_fresh19 = int(1)
    }
    return c2da_fresh18 * int(10) + c2da_fresh19
}
[export]
def neg_zero_and_unsigned() : int {
    var u : uint = uint(0)
    var d_6 : double = 0.0lf
    var f_1 : float = 0.0
    u = uint(0xee6b2800)
    d_6 = double(u)
    f_1 = -0.0
    var c2da_fresh20 : int = int(0)
    if (d_6 > 3900000000.0lf) {
        c2da_fresh20 = int(1)
    }
    var c2da_fresh22 : int = 0
    if (c2da_fresh20 != 0) {
        var c2da_fresh21 : int = int(0)
        if (f_1 == 0.0) {
            c2da_fresh21 = int(1)
        }
        if (c2da_fresh21 != 0) {
            c2da_fresh22 = 1
        }
    }
    return c2da_fresh22
}
[export]
def half(var x_0 : double) : double {
    return x_0 / double(int(2))
}
[export]
def halff(var x_1 : float) : float {
    return x_1 / float(int(2))
}
[export]
def calls() : int {
    var c2da_fresh23 : int = int(0)
    if (double(half(double(int(5)))) == 2.5lf) {
        c2da_fresh23 = int(1)
    }
    var c2da_fresh25 : int = 0
    if (c2da_fresh23 != 0) {
        var c2da_fresh24 : int = int(0)
        if (float(halff(float(int(5)))) == 2.5) {
            c2da_fresh24 = int(1)
        }
        if (c2da_fresh24 != 0) {
            c2da_fresh25 = 1
        }
    }
    return c2da_fresh25
}
[export]
def floating_runtime() : int {
    if (int(dbl_literal()) == 0) {
        goto label 10
    }
    if (int(dbl_exact()) == 0) {
        goto label 9
    }
    if (int(dbl_third()) == 0) {
        goto label 8
    }
    var c2da_fresh26 : int = int(0)
    if (int(cmp_zero()) != int(1111)) {
        c2da_fresh26 = int(1)
    }
    if (c2da_fresh26 != 0) {
        goto label 7
    }
    if (int(flt()) == 0) {
        goto label 6
    }
    var c2da_fresh27 : int = int(0)
    if (int(not_dbl()) != int(11)) {
        c2da_fresh27 = int(1)
    }
    if (c2da_fresh27 != 0) {
        goto label 5
    }
    var c2da_fresh28 : int = int(0)
    if (int(conv()) != -int(165)) {
        c2da_fresh28 = int(1)
    }
    if (c2da_fresh28 != 0) {
        goto label 4
    }
    var c2da_fresh29 : int = int(0)
    if (int(while_dbl()) != int(7)) {
        c2da_fresh29 = int(1)
    }
    if (c2da_fresh29 != 0) {
        goto label 3
    }
    var c2da_fresh30 : int = int(0)
    if (int(mixed()) != int(11)) {
        c2da_fresh30 = int(1)
    }
    if (c2da_fresh30 != 0) {
        goto label 2
    }
    if (int(neg_zero_and_unsigned()) == 0) {
        goto label 1
    }
    if (int(calls()) == 0) {
        goto label 0
    }
    return int(0)
    label 0:
    return int(11)
    label 1:
    return int(10)
    label 2:
    return int(9)
    label 3:
    return int(8)
    label 4:
    return int(7)
    label 5:
    return int(6)
    label 6:
    return int(5)
    label 7:
    return int(4)
    label 8:
    return int(3)
    label 9:
    return int(2)
    label 10:
    return int(1)
}
