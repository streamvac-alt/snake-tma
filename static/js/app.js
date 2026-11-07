const tg = window.Telegram?.WebApp;
tg?.ready(); tg?.expand();

const user = tg?.initDataUnsafe?.user;
const userId = user ? user.id : 0;
const userName = user ? user.first_name : 'Гость';
document.getElementById('greeting').textContent = `Привет, ${userName}!`;

let categories = [], products = [], activeCategoryId = null, cart = [], selectedProduct = null, statusInterval = null, orderStartTime = null;

async function init() {
    try {
        const res = await fetch('/api/menu');
        const data = await res.json();
        categories = data.categories;
        products = data.products;
        
        renderCategories();
        if (categories.length > 0) selectCategory(categories[0].id);
        checkActiveOrder();
    } catch (e) {
        document.getElementById('products-area').innerHTML = '<p style="color:red;text-align:center;padding:20px;">Ошибка загрузки меню</p>';
    }
}

function renderCategories() {
    const container = document.getElementById('categories-scroll');
    container.innerHTML = categories.map(c => `
        <button class="category-pill" onclick="selectCategory(${c.id})" id="cat-${c.id}">${c.name}</button>
    `).join('');
}

function selectCategory(id) {
    activeCategoryId = id;
    document.querySelectorAll('.category-pill').forEach(b => b.classList.remove('active'));
    document.getElementById(`cat-${id}`).classList.add('active');
    
    const area = document.getElementById('products-area');
    const cat = categories.find(c => c.id === activeCategoryId);
    const filtered = products.filter(p => p.category_id === activeCategoryId);

    if (filtered.length === 0) {
        area.innerHTML = `<div class="products-section"><div class="empty" style="text-align:center;color:#a1a1a1;padding:20px;">В этой категории пока пусто</div></div>`;
        return;
    }

    // Формируем секцию с карточками
    area.innerHTML = `
        <div class="products-section">
            <div class="section-title">${cat.name}</div>
            <div class="products-row">
                ${filtered.map(p => {
                    const img = p.image_url || 'https://via.placeholder.com/150/262626/FFFFFF?text=Food';
                    return `
                    <div class="product-card" onclick="openProduct(${p.id})">
                        <img src="${img}" class="product-img" onerror="this.src='https://via.placeholder.com/150/262626/FFFFFF?text=Food'">
                        <div class="product-title">${p.name}</div>
                        <div class="product-price">от ${p.variants[0].price} GEL</div>
                        <button class="btn-add">Выбрать</button>
                    </div>`;
                }).join('')}
            </div>
        </div>
    `;
}

function openProduct(id) {
    selectedProduct = products.find(p => p.id === id);
    if (!selectedProduct) return;

    if (selectedProduct.variants.length === 1) {
        addToCart(selectedProduct, selectedProduct.variants[0]);
        return;
    }

    document.getElementById('size-modal-title').textContent = selectedProduct.name;
    document.getElementById('size-options').innerHTML = selectedProduct.variants.map(v => `
        <div class="variant-option" onclick='addToCart(selectedProduct, ${JSON.stringify(v)})'>
            <span>${v.name}</span><span>${v.price} GEL</span>
        </div>
    `).join('');
    openModal('size-modal');
}

function addToCart(product, variant) {
    const existing = cart.find(i => i.variant.id === variant.id);
    if (existing) existing.qty += 1; else cart.push({ product, variant, qty: 1 });
    updateCartUI(); closeModal('size-modal');
    if (tg) tg.HapticFeedback.impactOccurred('light');
}

function updateCartUI() {
    let total = 0, count = 0;
    cart.forEach(i => { total += i.variant.price * i.qty; count += i.qty; });
    document.getElementById('cart-total').textContent = `${total} GEL`;
    document.getElementById('cart-fab').classList.toggle('visible', count > 0);
    document.getElementById('cart-items').innerHTML = cart.map((i, index) => `
        <div class="cart-item">
            <div class="cart-item-info"><h4>${i.product.name}</h4><p>${i.variant.name}</p></div>
            <div class="cart-controls">
                <button onclick="changeQty(${index}, -1)">-</button><span>${i.qty}</span><button onclick="changeQty(${index}, 1)">+</button>
            </div>
        </div>
    `).join('');
}

function changeQty(index, delta) {
    cart[index].qty += delta;
    if (cart[index].qty <= 0) cart.splice(index, 1);
    updateCartUI();
}

async function submitOrder() {
    if (cart.length === 0) return;
    const items = cart.map(i => ({ variant_id: i.variant.id, qty: i.qty }));
    const res = await fetch('/api/order', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ user_id: userId, user_name: userName, items, comment: document.getElementById('order-comment').value })
    });
    const data = await res.json();
    if (data.ok) {
        cart = []; updateCartUI(); closeModal('cart-modal'); document.getElementById('order-comment').value = '';
        showStatus(data.order_number);
    } else alert(data.error || 'Ошибка');
}

function showStatus(orderNum) {
    document.getElementById('status-screen').style.display = 'flex';
    document.getElementById('status-order-num').textContent = `№${orderNum}`;
    orderStartTime = Date.now();
    document.getElementById('status-title').textContent = 'Ожидает принятия';
    document.getElementById('btn-new-order').style.display = 'none';
    statusInterval = setInterval(checkOrderStatus, 2000);
}

async function checkOrderStatus() {
    const res = await fetch(`/api/orders`);
    const orders = await res.json();
    const myOrder = orders.find(o => o.user_id === userId);
    if (myOrder) {
        if (myOrder.status === 'accepted') document.getElementById('status-title').textContent = 'Готовится';
        else if (myOrder.status === 'ready') {
            document.getElementById('status-title').textContent = 'Готов к выдаче!';
            document.getElementById('btn-new-order').style.display = 'block';
            clearInterval(statusInterval);
            if (tg) tg.HapticFeedback.notificationOccurred('success');
        }
    }
    const elapsed = Math.floor((Date.now() - orderStartTime) / 1000);
    document.getElementById('status-timer').textContent = `${String(Math.floor(elapsed / 60)).padStart(2, '0')}:${String(elapsed % 60).padStart(2, '0')}`;
}

async function checkActiveOrder() {
    if (!userId) return;
    const res = await fetch(`/api/orders`);
    const orders = await res.json();
    const myOrder = orders.find(o => o.user_id === userId);
    if (myOrder) showStatus(myOrder.order_number);
}

function resetToMenu() { document.getElementById('status-screen').style.display = 'none'; }
function openCart() { openModal('cart-modal'); }
function openModal(id) { document.getElementById(id).classList.add('open'); }
function closeModal(id) { document.getElementById(id).classList.remove('open'); }

init();
