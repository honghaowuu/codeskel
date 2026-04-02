from utils import StringUtils

class UserService:
    def greet(self, name: str) -> str:
        u = StringUtils()
        return u.capitalize(name)
