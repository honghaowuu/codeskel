package com.example.service;
import com.example.model.User;
import com.example.repo.UserRepository;

public class UserService {
    private UserRepository repo;

    public User findUser(String id) {
        User user = repo.findById(id);
        return user;
    }

    public void saveUser(User user) {
        repo.save(user);
    }

    public String getUserEmail(String id) {
        User user = repo.findById(id);
        return user.getEmail();
    }
}
