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
var c2da_str_0 : array<int8> = [int8(97), int8(34), int8(98), int8(92), int8(99), int8(10), int8(9), int8(0)]

var c2da_str_1 : array<int8> = [int8(-1), int8(1), int8(0)]

var c2da_str_2 : array<int8> = [int8(104), int8(101), int8(108), int8(108), int8(111), int8(44), int8(32), int8(119), int8(111), int8(114), int8(108), int8(100), int8(0)]

var c2da_str_3 : array<int8> = [int8(0)]

def c2da_add_overflow_int(a : int; b : int; var res : int?) : bool {
    var wide : int64 = int64(a) + int64(b)
    unsafe {
        *res = int(wide)
    }
    return int64(int(wide)) != wide
}
def c2da_bswap16(v : uint) : uint {
    return v << uint(0x8) & uint(0xff00) | v >> uint(0x8) & uint(0xff)
}
def c2da_bswap32(v : uint) : uint {
    return v << uint(0x18) & uint(0xff000000) | v << uint(0x8) & uint(0xff0000) | v >> uint(0x8) & uint(0xff00) | v >> uint(0x18) & uint(0xff)
}
def c2da_ffs_int(v : int) : int {
    if (v == int(0)) {
        return 0
    }
    return int(ctz(uint(v))) + 1
}
def c2da_mul_overflow_int(a : int; b : int; var res : int?) : bool {
    var wide : int64 = int64(a) * int64(b)
    unsafe {
        *res = int(wide)
    }
    return int64(int(wide)) != wide
}
[export]
def unary_prec(var a : int; var b : int) : int {
    var c2da_fresh0 : int = int(0)
    if (a != b) {
        c2da_fresh0 = int(1)
    }
    var c2da_fresh1 : int = int(0)
    if (c2da_fresh0 == 0) {
        c2da_fresh1 = int(1)
    }
    return -(a + b) + ~(a | b) + c2da_fresh1
}
[export]
def shift_mix(var x : uint) : int {
    return int((x >> uint(int(4)) & uint(int(15))) + (~(x & uint(int(48))) >> uint(int(4)) & uint(int(15))))
}
[export]
def cast_chain(var v : int64) : int {
    return int(uint8(v + int64(int(1))))
}
[export]
def dbl_paren(var a_0 : double; var b_0 : double) : int {
    return int(-(a_0 * b_0) / 2.0lf)
}
[export]
def str_escape() : int {
    var s : int8 const? = null
    s = unsafe(addr(c2da_str_0[0]))
    var c2da_fresh2 : int = int(0)
    if (int(unsafe(unsafe(unsafe(reinterpret<int8 const?>(s)))[int(1)])) == 34) {
        c2da_fresh2 = int(1)
    }
    var c2da_fresh4 : int = 0
    if (c2da_fresh2 != 0) {
        var c2da_fresh3 : int = int(0)
        if (int(unsafe(unsafe(unsafe(reinterpret<int8 const?>(s)))[int(3)])) == 92) {
            c2da_fresh3 = int(1)
        }
        if (c2da_fresh3 != 0) {
            c2da_fresh4 = 1
        }
    }
    var c2da_fresh6 : int = 0
    if (c2da_fresh4 != 0) {
        var c2da_fresh5 : int = int(0)
        if (int(unsafe(unsafe(unsafe(reinterpret<int8 const?>(s)))[int(5)])) == 10) {
            c2da_fresh5 = int(1)
        }
        if (c2da_fresh5 != 0) {
            c2da_fresh6 = 1
        }
    }
    var c2da_fresh8 : int = 0
    if (c2da_fresh6 != 0) {
        var c2da_fresh7 : int = int(0)
        if (int(unsafe(unsafe(unsafe(reinterpret<int8 const?>(s)))[int(6)])) == 9) {
            c2da_fresh7 = int(1)
        }
        if (c2da_fresh7 != 0) {
            c2da_fresh8 = 1
        }
    }
    var c2da_fresh10 : int = 0
    if (c2da_fresh8 != 0) {
        var c2da_fresh9 : int = int(0)
        if (int(unsafe(unsafe(unsafe(reinterpret<int8 const?>(s)))[int(7)])) == int(0)) {
            c2da_fresh9 = int(1)
        }
        if (c2da_fresh9 != 0) {
            c2da_fresh10 = 1
        }
    }
    return c2da_fresh10
}
[export]
def str_bytes() : int {
    var s_0 : int8 const? = null
    s_0 = unsafe(addr(c2da_str_1[0]))
    var c2da_fresh11 : int = int(0)
    if (int(uint8(unsafe(unsafe(unsafe(reinterpret<int8 const?>(s_0)))[int(0)]))) == int(255)) {
        c2da_fresh11 = int(1)
    }
    var c2da_fresh13 : int = 0
    if (c2da_fresh11 != 0) {
        var c2da_fresh12 : int = int(0)
        if (int(unsafe(unsafe(unsafe(reinterpret<int8 const?>(s_0)))[int(1)])) == int(1)) {
            c2da_fresh12 = int(1)
        }
        if (c2da_fresh12 != 0) {
            c2da_fresh13 = 1
        }
    }
    return c2da_fresh13
}
[export]
def char_lit() : int {
    var c : int8 = int8(0)
    c = int8(-1)
    var c2da_fresh14 : int = int(0)
    if (int(c) == -int(1)) {
        c2da_fresh14 = int(1)
    }
    var c2da_fresh16 : int = 0
    if (c2da_fresh14 != 0) {
        var c2da_fresh15 : int = int(0)
        if (65 == int(65)) {
            c2da_fresh15 = int(1)
        }
        if (c2da_fresh15 != 0) {
            c2da_fresh16 = 1
        }
    }
    var c2da_fresh18 : int = 0
    if (c2da_fresh16 != 0) {
        var c2da_fresh17 : int = int(0)
        if (10 == int(10)) {
            c2da_fresh17 = int(1)
        }
        if (c2da_fresh17 != 0) {
            c2da_fresh18 = 1
        }
    }
    var c2da_fresh20 : int = 0
    if (c2da_fresh18 != 0) {
        var c2da_fresh19 : int = int(0)
        if (0 == int(0)) {
            c2da_fresh19 = int(1)
        }
        if (c2da_fresh19 != 0) {
            c2da_fresh20 = 1
        }
    }
    return c2da_fresh20
}
[export]
def str_len(var s_1 : int8 const?) : int {
    var n : int = int(0)
    n = int(0)
    label 0:
    if (*s_1 != 0) {
        goto label 1
    }
    return n
    label 1:
    var c2da_postinc : int8 const? = s_1
    s_1 = unsafe(s_1 + 1)
    c2da_postinc
    var c2da_postinc_0 : int = n
    n = int(n + 1)
    c2da_postinc_0
    goto label 0
    panic("unreachable: fell out of a translated control-flow graph")
}
[export]
def builtins() : int {
    var c2da_fresh21 : int = int(0)
    if (int(popcnt(uint(uint(0xff)))) == int(8)) {
        c2da_fresh21 = int(1)
    }
    var c2da_fresh23 : int = 0
    if (c2da_fresh21 != 0) {
        var c2da_fresh22 : int = int(0)
        if (int(clz(uint(uint(0x1)))) == int(31)) {
            c2da_fresh22 = int(1)
        }
        if (c2da_fresh22 != 0) {
            c2da_fresh23 = 1
        }
    }
    var c2da_fresh25 : int = 0
    if (c2da_fresh23 != 0) {
        var c2da_fresh24 : int = int(0)
        if (int(ctz(uint(uint(0x8)))) == int(3)) {
            c2da_fresh24 = int(1)
        }
        if (c2da_fresh24 != 0) {
            c2da_fresh25 = 1
        }
    }
    var c2da_fresh27 : int = 0
    if (c2da_fresh25 != 0) {
        var c2da_fresh26 : int = int(0)
        if (c2da_bswap32(uint(uint(0x11223344))) == uint(0x44332211)) {
            c2da_fresh26 = int(1)
        }
        if (c2da_fresh26 != 0) {
            c2da_fresh27 = 1
        }
    }
    var c2da_fresh29 : int = 0
    if (c2da_fresh27 != 0) {
        var c2da_fresh28 : int = int(0)
        if (c2da_ffs_int(int(int(8))) == int(4)) {
            c2da_fresh28 = int(1)
        }
        if (c2da_fresh28 != 0) {
            c2da_fresh29 = 1
        }
    }
    var c2da_fresh31 : int = 0
    if (c2da_fresh29 != 0) {
        var c2da_fresh30 : int = int(0)
        if (int(uint16(c2da_bswap16(uint(uint16(int(4660)))))) == int(13330)) {
            c2da_fresh30 = int(1)
        }
        if (c2da_fresh30 != 0) {
            c2da_fresh31 = 1
        }
    }
    return c2da_fresh31
}
[export]
def overflow() : int {
    var r : int = int(0)
    var o : int = int(0)
    var r2 : int = int(0)
    var o2 : int = int(0)
    var c2da_fresh32 : int = int(0)
    if (c2da_add_overflow_int(int(int(2147483647)), int(int(1)), unsafe(addr(r))) == true) {
        c2da_fresh32 = int(1)
    }
    o = c2da_fresh32
    var c2da_fresh33 : int = int(0)
    if (c2da_mul_overflow_int(int(int(1000)), int(int(1000)), unsafe(addr(r2))) == true) {
        c2da_fresh33 = int(1)
    }
    o2 = c2da_fresh33
    var c2da_fresh34 : int = int(0)
    if (o == int(1)) {
        c2da_fresh34 = int(1)
    }
    var c2da_fresh36 : int = 0
    if (c2da_fresh34 != 0) {
        var c2da_fresh35 : int = int(0)
        if (o2 == int(0)) {
            c2da_fresh35 = int(1)
        }
        if (c2da_fresh35 != 0) {
            c2da_fresh36 = 1
        }
    }
    var c2da_fresh38 : int = 0
    if (c2da_fresh36 != 0) {
        var c2da_fresh37 : int = int(0)
        if (r2 == int(1000000)) {
            c2da_fresh37 = int(1)
        }
        if (c2da_fresh37 != 0) {
            c2da_fresh38 = 1
        }
    }
    return c2da_fresh38
}
[export]
def expect_0(var x_0 : int) : int {
    var c2da_fresh39 : int = int(0)
    if (x_0 == int(3)) {
        c2da_fresh39 = int(1)
    }
    if (int64(c2da_fresh39) != 0) {
        goto label 0
    }
    return int(0)
    label 0:
    return int(1)
}
[export]
def neg_lit() : int {
    var a_1 : int = int(0)
    var b_1 : uint = uint(0)
    var c_0 : int64 = int64(0)
    a_1 = -int(5)
    b_1 = uint(0xffffffff)
    c_0 = -int64(0x7fffffffffffffffuL) - int64(int(1))
    var c2da_fresh40 : int = int(0)
    if (a_1 == -int(5)) {
        c2da_fresh40 = int(1)
    }
    var c2da_fresh42 : int = 0
    if (c2da_fresh40 != 0) {
        var c2da_fresh41 : int = int(0)
        if (b_1 == uint(0xffffffff)) {
            c2da_fresh41 = int(1)
        }
        if (c2da_fresh41 != 0) {
            c2da_fresh42 = 1
        }
    }
    var c2da_fresh44 : int = 0
    if (c2da_fresh42 != 0) {
        var c2da_fresh43 : int = int(0)
        if (c_0 < int64(int(0))) {
            c2da_fresh43 = int(1)
        }
        if (c2da_fresh43 != 0) {
            c2da_fresh44 = 1
        }
    }
    return c2da_fresh44
}
[export]
def paren_assign(var a_2 : int; var b_2 : int) : int {
    var r_0 : int = int(0)
    a_2 = b_2
    r_0 = a_2 + int(1)
    return r_0 * int(10) + a_2
}
[export]
def printer_runtime() : int {
    var c2da_fresh45 : int = int(0)
    if (int(unary_prec(int(1), int(2))) != -int(7)) {
        c2da_fresh45 = int(1)
    }
    if (c2da_fresh45 != 0) {
        goto label 12
    }
    var c2da_fresh46 : int = int(0)
    if (int(shift_mix(uint(0x35))) != int(15)) {
        c2da_fresh46 = int(1)
    }
    if (c2da_fresh46 != 0) {
        goto label 11
    }
    var c2da_fresh47 : int = int(0)
    if (int(cast_chain(int64(int(254)))) != int(255)) {
        c2da_fresh47 = int(1)
    }
    if (c2da_fresh47 != 0) {
        goto label 10
    }
    var c2da_fresh48 : int = int(0)
    if (int(dbl_paren(3.0lf, 4.0lf)) != -int(6)) {
        c2da_fresh48 = int(1)
    }
    if (c2da_fresh48 != 0) {
        goto label 9
    }
    if (int(str_escape()) == 0) {
        goto label 8
    }
    if (int(str_bytes()) == 0) {
        goto label 7
    }
    if (int(char_lit()) == 0) {
        goto label 6
    }
    var c2da_fresh49 : int = int(0)
    if (int(str_len(unsafe(unsafe(reinterpret<int8 const?>(unsafe(addr(c2da_str_2[0]))))))) != int(12)) {
        c2da_fresh49 = int(1)
    }
    var c2da_fresh51 : int = 1
    if (!(c2da_fresh49 != 0)) {
        var c2da_fresh50 : int = int(0)
        if (int(str_len(unsafe(unsafe(reinterpret<int8 const?>(unsafe(addr(c2da_str_3[0]))))))) != int(0)) {
            c2da_fresh50 = int(1)
        }
        if (!(c2da_fresh50 != 0)) {
            c2da_fresh51 = 0
        }
    }
    if (c2da_fresh51 != 0) {
        goto label 5
    }
    if (int(builtins()) == 0) {
        goto label 4
    }
    if (int(overflow()) == 0) {
        goto label 3
    }
    var c2da_fresh52 : int = int(0)
    if (int(expect_0(int(3))) != int(1)) {
        c2da_fresh52 = int(1)
    }
    var c2da_fresh54 : int = 1
    if (!(c2da_fresh52 != 0)) {
        var c2da_fresh53 : int = int(0)
        if (int(expect_0(int(4))) != int(0)) {
            c2da_fresh53 = int(1)
        }
        if (!(c2da_fresh53 != 0)) {
            c2da_fresh54 = 0
        }
    }
    if (c2da_fresh54 != 0) {
        goto label 2
    }
    if (int(neg_lit()) == 0) {
        goto label 1
    }
    var c2da_fresh55 : int = int(0)
    if (int(paren_assign(int(1), int(4))) != int(54)) {
        c2da_fresh55 = int(1)
    }
    if (c2da_fresh55 != 0) {
        goto label 0
    }
    return int(0)
    label 0:
    return int(13)
    label 1:
    return int(12)
    label 2:
    return int(11)
    label 3:
    return int(10)
    label 4:
    return int(9)
    label 5:
    return int(8)
    label 6:
    return int(7)
    label 7:
    return int(6)
    label 8:
    return int(5)
    label 9:
    return int(4)
    label 10:
    return int(3)
    label 11:
    return int(2)
    label 12:
    return int(1)
}
