const { promisify } = require("util")
const addon = require('../native')

const Native = Symbol()

class Client {
    constructor() {
        Object.defineProperty(this, Native, {
            value: new addon.Client(arguments),
            enumerable: false
        })
    }

    get(url) {
        return new Promise((resolve, reject) => this[Native].get(url, (err, res) => err != null ? reject(err) : resolve(res)))
    }
}

module.exports = {
    Client,
    get: promisify(addon.get)
}